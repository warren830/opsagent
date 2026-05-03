//! War-room automation — given an incident id, spin up the Slack channel,
//! cut a Jira ticket, invite the owner-group responders, and write the
//! resulting refs back onto the incident row.
//!
//! This runs inside a `tokio::spawn` from `lifecycle::create_incident_with_automation`,
//! so we never fail the outer handler. Every step appends to
//! `WarRoomResult::errors` on failure and moves on.

use chrono::Utc;
use futures::stream::{self, StreamExt};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

use crate::models::channel::Channel;
use crate::models::incident::Incident;
use crate::models::user::User;
use crate::services::incident::{slack_helper, timeline, timeline_bus::TimelineBus};
use crate::services::jira::JiraClient;

pub struct WarRoomResult {
    pub channel_ref: Option<serde_json::Value>,
    pub jira_key: Option<String>,
    pub errors: Vec<String>,
}

/// Full pipeline: channel → invites → Jira → DB writeback → timeline event.
/// Errors are collected into `WarRoomResult::errors` and logged at WARN
/// level; the caller does not need to branch on them.
pub async fn spawn_war_room(
    pool: &PgPool,
    bus: Arc<TimelineBus>,
    incident_id: Uuid,
) -> WarRoomResult {
    let mut result = WarRoomResult {
        channel_ref: None,
        jira_key: None,
        errors: Vec::new(),
    };

    // ---- 1. Load incident ----------------------------------------------------
    let incident = match sqlx::query_as::<_, Incident>(
        "SELECT * FROM incidents WHERE id = $1",
    )
    .bind(incident_id)
    .fetch_optional(pool)
    .await
    {
        Ok(Some(i)) => i,
        Ok(None) => {
            result.errors.push(format!("incident {incident_id} not found"));
            return result;
        }
        Err(e) => {
            result.errors.push(format!("load incident failed: {e}"));
            return result;
        }
    };

    // ---- 2. Slack war room ---------------------------------------------------
    match slack_helper::get_slack_token(pool, incident.tenant_id).await {
        Ok(Some(token)) => {
            let base_name = build_channel_name(&incident.title, Utc::now());
            // P1 #16: if the base channel name is already taken (duplicate
            // incident title in the same day, replay, etc.), retry once
            // with the incident number appended. Anything beyond that
            // collision is recorded as an error — we stop the spiral here
            // rather than looping forever.
            let create_result = match slack_helper::slack_create_channel(&token, &base_name).await {
                Ok(ch) => Ok(ch),
                Err(crate::error::AppError::Conflict(_)) => {
                    let retry_name = format!("{}-{}", base_name, incident.number);
                    // Slack caps channel names at 80 chars; truncate the
                    // retry name to stay inside that bound.
                    let retry_name = if retry_name.len() > 80 {
                        retry_name.chars().take(80).collect::<String>()
                    } else {
                        retry_name
                    };
                    slack_helper::slack_create_channel(&token, &retry_name).await
                }
                Err(e) => Err(e),
            };
            match create_result {
                Ok(ch) => {
                    let url = format!("https://slack.com/app_redirect?channel={}", ch.id);
                    let channel_ref = serde_json::json!({
                        "channel_id": ch.id,
                        "platform_channel_id": ch.id,
                        "name": ch.name,
                        "url": url,
                    });

                    let initial_text = format!(
                        ":rotating_light: *Incident #{}* — {}\nSeverity: *{}*\nStatus: *{}*\nImpact: {}",
                        incident.number,
                        incident.title,
                        incident.severity,
                        incident.status,
                        incident.impact_summary.as_deref().unwrap_or("(pending)"),
                    );
                    if let Err(e) =
                        slack_helper::slack_post_message(&token, &ch.id, &initial_text).await
                    {
                        result.errors.push(format!("slack postMessage: {e}"));
                    }

                    // Invite owner-group members.
                    match resolve_responder_user_ids(pool, &token, &incident).await {
                        Ok(ids) if !ids.is_empty() => {
                            if let Err(e) =
                                slack_helper::slack_invite_users(&token, &ch.id, &ids).await
                            {
                                result.errors.push(format!("slack invite: {e}"));
                            }
                        }
                        Ok(_) => {
                            tracing::debug!(
                                "war_room: no Slack users resolvable for incident {}",
                                incident.id
                            );
                        }
                        Err(e) => {
                            result.errors.push(format!("resolve responders: {e}"));
                        }
                    }

                    result.channel_ref = Some(channel_ref);
                }
                Err(e) => {
                    result.errors.push(format!("slack create: {e}"));
                }
            }
        }
        Ok(None) => {
            tracing::info!(
                "war_room: no Slack integration for tenant {:?} — skipping channel",
                incident.tenant_id
            );
        }
        Err(e) => {
            result.errors.push(format!("slack token lookup: {e}"));
        }
    }

    // ---- 3. Jira ticket ------------------------------------------------------
    match fetch_jira_client(pool, incident.tenant_id).await {
        Ok(Some(client)) => {
            let summary = format!("[INC-{}] {}", incident.number, incident.title);
            let description = format!(
                "Auto-created from Ops Incident Command Center.\n\nSeverity: {}\nStatus: {}\nImpact: {}\nDetection source: {}",
                incident.severity,
                incident.status,
                incident.impact_summary.as_deref().unwrap_or("(pending)"),
                incident.detection_source,
            );
            let labels = Some(vec![
                "ops-incident".to_string(),
                format!("sev-{}", incident.severity.trim_start_matches("sev")),
            ]);
            match client.create_issue(&summary, &description, None, labels).await {
                Ok(issue) => {
                    result.jira_key = Some(issue.key);
                }
                Err(e) => {
                    result.errors.push(format!("jira create: {e}"));
                }
            }
        }
        Ok(None) => {
            tracing::info!(
                "war_room: no Jira integration for tenant {:?} — skipping ticket",
                incident.tenant_id
            );
        }
        Err(e) => {
            result.errors.push(format!("jira client: {e}"));
        }
    }

    // ---- 4. Write back + timeline -------------------------------------------
    if let Err(e) = sqlx::query(
        r#"UPDATE incidents SET
               war_room_channel_ref = COALESCE($2, war_room_channel_ref),
               jira_key = COALESCE($3, jira_key),
               updated_at = NOW()
           WHERE id = $1"#,
    )
    .bind(incident_id)
    .bind(&result.channel_ref)
    .bind(&result.jira_key)
    .execute(pool)
    .await
    {
        result.errors.push(format!("writeback: {e}"));
    }

    let actor = serde_json::json!({
        "kind": "system",
        "source": "war_room",
    });
    let mut summary_parts: Vec<String> = Vec::new();
    if let Some(ref r) = result.channel_ref {
        let name = r.get("name").and_then(|v| v.as_str()).unwrap_or("?");
        summary_parts.push(format!("Slack #{name}"));
    }
    if let Some(ref key) = result.jira_key {
        summary_parts.push(format!("Jira {key}"));
    }
    let summary = if summary_parts.is_empty() {
        "War-room automation ran with no integrations configured".to_string()
    } else {
        format!("War room ready: {}", summary_parts.join(", "))
    };

    if let Err(e) = timeline::record_event(
        pool,
        &bus,
        incident_id,
        timeline::KIND_STATUS_CHANGED,
        actor,
        &summary,
        serde_json::json!({
            "war_room_channel_ref": result.channel_ref,
            "jira_key": result.jira_key,
            "errors": result.errors,
        }),
    )
    .await
    {
        tracing::warn!("war_room timeline write failed: {e}");
    }

    result
}

// ---------------------------------------------------------------------------
// Helpers.
// ---------------------------------------------------------------------------

/// Build `#inc-YYYYMMDD-<kebab-title>` within Slack's name rules:
/// lower-case, `[a-z0-9._-]+`, max 80 chars. We truncate the title portion
/// to 40 chars so the date prefix always fits.
pub fn build_channel_name(title: &str, now: chrono::DateTime<Utc>) -> String {
    let date = now.format("%Y%m%d").to_string();

    // Kebab-ize: lowercase ASCII alnum + '-', collapse runs.
    let mut kebab = String::with_capacity(40);
    let mut last_dash = false;
    for ch in title.chars() {
        let c = ch.to_ascii_lowercase();
        if c.is_ascii_alphanumeric() {
            kebab.push(c);
            last_dash = false;
        } else if !last_dash && !kebab.is_empty() {
            kebab.push('-');
            last_dash = true;
        }
    }
    // Strip trailing dash + truncate to 40.
    let trimmed: String = kebab.trim_matches('-').chars().take(40).collect();
    let trimmed = trimmed.trim_end_matches('-');
    let body = if trimmed.is_empty() { "incident" } else { trimmed };

    format!("inc-{date}-{body}")
}

/// Return Slack user IDs for the members of the owner-group of each
/// affected component. This MVP reads `catalog_entities.annotations.members`
/// as a JSON array of user emails/usernames — we resolve those through the
/// `users` table to pull real email addresses and then through Slack's
/// `users.lookupByEmail`. If nothing is found we fall back to the incident
/// commander (if any).
async fn resolve_responder_user_ids(
    pool: &PgPool,
    slack_token: &str,
    incident: &Incident,
) -> Result<Vec<String>, sqlx::Error> {
    let mut emails: Vec<String> = Vec::new();

    // Pull owner-group annotations → emails/usernames.
    if !incident.affected_component_ids.is_empty() {
        let rows: Vec<(Option<Uuid>,)> = sqlx::query_as(
            r#"SELECT owner_group_id FROM catalog_entities
               WHERE id = ANY($1)"#,
        )
        .bind(&incident.affected_component_ids)
        .fetch_all(pool)
        .await?;

        let group_ids: Vec<Uuid> = rows.into_iter().filter_map(|(id,)| id).collect();
        if !group_ids.is_empty() {
            let members: Vec<(serde_json::Value,)> = sqlx::query_as(
                "SELECT annotations FROM catalog_entities
                 WHERE id = ANY($1) AND kind = 'group'",
            )
            .bind(&group_ids)
            .fetch_all(pool)
            .await?;

            for (ann,) in members {
                if let Some(arr) = ann.get("members").and_then(|v| v.as_array()) {
                    for m in arr {
                        if let Some(s) = m.as_str() {
                            emails.push(s.to_string());
                        }
                    }
                }
            }
        }
    }

    // Fallback: incident commander's email.
    if emails.is_empty()
        && let Some(cmd_id) = incident.commander_user_id
    {
        let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = $1")
            .bind(cmd_id)
            .fetch_optional(pool)
            .await?;
        if let Some(u) = user
            && let Some(email) = u.email
        {
            emails.push(email);
        }
    }

    // Resolve usernames → email addresses (DB hits, serial is fine — all
    // hit the same pool and the list is small).
    let mut resolved_emails: Vec<String> = Vec::with_capacity(emails.len());
    for email in emails {
        let resolved = if email.contains('@') {
            Some(email)
        } else {
            let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE username = $1")
                .bind(&email)
                .fetch_optional(pool)
                .await?;
            user.and_then(|u| u.email)
        };
        if let Some(e) = resolved {
            resolved_emails.push(e);
        }
    }

    // Translate emails → Slack user IDs concurrently. Slack's rate budget
    // is generous but we cap at 5 in flight so a slow lookup can't pile up
    // — best effort: failures are logged and skipped. P1 #15.
    let slack_ids: Vec<String> = stream::iter(resolved_emails)
        .map(|email| async move {
            match slack_helper::slack_lookup_user_by_email(slack_token, &email).await {
                Ok(Some(id)) => Some(id),
                Ok(None) => {
                    tracing::debug!("slack: no user for email {} (skipping)", email);
                    None
                }
                Err(e) => {
                    tracing::warn!("slack lookup {} failed: {}", email, e);
                    None
                }
            }
        })
        .buffer_unordered(5)
        .filter_map(|v| async move { v })
        .collect()
        .await;

    Ok(slack_ids)
}

/// Build a JiraClient for the tenant, returning `Ok(None)` if the tenant
/// does not have a Jira channel configured.
async fn fetch_jira_client(
    pool: &PgPool,
    tenant_id: Option<Uuid>,
) -> Result<Option<JiraClient>, sqlx::Error> {
    let channel = sqlx::query_as::<_, Channel>(
        "SELECT * FROM channels
         WHERE platform = 'jira' AND enabled = true
           AND ($1::UUID IS NULL OR tenant_id = $1)
         ORDER BY created_at ASC
         LIMIT 1",
    )
    .bind(tenant_id)
    .fetch_optional(pool)
    .await?;

    let Some(channel) = channel else {
        return Ok(None);
    };
    match JiraClient::from_credentials(&channel.credentials) {
        Ok(c) => Ok(Some(c)),
        Err(e) => {
            tracing::warn!("jira credentials invalid: {e}");
            Ok(None)
        }
    }
}

// ---------------------------------------------------------------------------
// Tests.
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn sample_date() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 5, 2, 12, 4, 0).unwrap()
    }

    #[test]
    fn channel_name_uses_date_prefix_and_kebab_body() {
        let name = build_channel_name("Checkout p99 spike", sample_date());
        assert_eq!(name, "inc-20260502-checkout-p99-spike");
    }

    #[test]
    fn channel_name_strips_non_ascii_and_truncates() {
        let long = "Really Long Title with Emoji 🔥 and Unicode «Test» that goes on and on forever";
        let name = build_channel_name(long, sample_date());
        assert!(name.starts_with("inc-20260502-"));
        // Body after the date prefix cannot exceed 40 chars.
        let body = name.strip_prefix("inc-20260502-").unwrap();
        assert!(body.len() <= 40, "body too long: {} ({})", body, body.len());
        // Lower-case ASCII + dash only.
        for ch in body.chars() {
            assert!(
                ch.is_ascii_alphanumeric() || ch == '-',
                "illegal char {ch} in {name}"
            );
        }
    }

    #[test]
    fn channel_name_falls_back_when_title_is_empty() {
        let name = build_channel_name("", sample_date());
        assert_eq!(name, "inc-20260502-incident");
    }

    #[test]
    fn channel_name_collapses_consecutive_separators() {
        let name = build_channel_name("A///B   C", sample_date());
        assert_eq!(name, "inc-20260502-a-b-c");
    }

    #[test]
    fn channel_name_trims_trailing_dash_after_truncation() {
        // Exactly 40 alnum chars followed by a separator — ensure no
        // trailing dash in the final 40-char slice.
        let title = format!("{} end", "a".repeat(40));
        let name = build_channel_name(&title, sample_date());
        let body = name.strip_prefix("inc-20260502-").unwrap();
        assert!(!body.ends_with('-'), "body {body} has trailing dash");
    }
}
