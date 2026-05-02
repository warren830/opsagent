//! Postmortem drafter — turns a resolved incident + its timeline into a
//! structured markdown postmortem draft.
//!
//! The draft is deliberately template-driven for MVP: the service
//! aggregates everything the UI already shows (incident row + timeline
//! events + deployment_events inside the incident window + severity
//! history) into a Google-SRE-style postmortem with all the standard
//! sections pre-filled from structured data. Free-text sections (Summary,
//! Root Cause, Lessons Learned) are left as TODO placeholders the IC
//! fills in during review.
//!
//! Why template instead of spawning Claude CLI here: the backend has no
//! interactive shell surface for a one-shot LLM call, and the agent
//! copilot in the war room already has full context to refine the draft
//! on demand. See the commit message for the W5-W6 batch for the
//! follow-up that layers Opus-driven rewriting on top of this scaffold.
//!
//! Action items are extracted from the `## Action Items` section of the
//! markdown using a simple table-row regex so downstream callers can
//! forward them to Jira without re-parsing the entire document.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::models::deployment_event::DeploymentEvent;
use crate::models::incident::{
    Incident, IncidentParticipant, IncidentSeverityHistory, IncidentTimelineEvent,
};

/// Result of a drafting run. `markdown` is the human-editable body that
/// will be persisted to `knowledge_files`; `action_items` are pulled out
/// so callers can create Jira tickets without re-parsing the doc.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostmortemDraft {
    pub markdown: String,
    pub action_items: Vec<ActionItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionItem {
    pub owner: Option<String>,
    pub description: String,
    pub due_date: Option<chrono::NaiveDate>,
}

/// Build a postmortem draft for `incident_id`.
///
/// Returns `NotFound` if the incident does not exist. Collects the
/// timeline, severity history, participants, and deployment_events that
/// intersect the incident's active window, then formats the data as
/// markdown.
pub async fn draft(pool: &PgPool, incident_id: Uuid) -> AppResult<PostmortemDraft> {
    let inc = sqlx::query_as::<_, Incident>("SELECT * FROM incidents WHERE id = $1")
        .bind(incident_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Incident not found".to_string()))?;

    let timeline = sqlx::query_as::<_, IncidentTimelineEvent>(
        r#"SELECT * FROM incident_timeline_events
           WHERE incident_id = $1
           ORDER BY occurred_at ASC"#,
    )
    .bind(incident_id)
    .fetch_all(pool)
    .await?;

    let severity_history = sqlx::query_as::<_, IncidentSeverityHistory>(
        r#"SELECT * FROM incident_severity_history
           WHERE incident_id = $1
           ORDER BY changed_at ASC"#,
    )
    .bind(incident_id)
    .fetch_all(pool)
    .await?;

    let participants = sqlx::query_as::<_, IncidentParticipant>(
        r#"SELECT * FROM incident_participants
           WHERE incident_id = $1
           ORDER BY joined_at ASC"#,
    )
    .bind(incident_id)
    .fetch_all(pool)
    .await?;

    // Deployment events that overlap the incident's active window. We
    // filter by the incident's affected components to keep the noise low
    // — a deploy on an unrelated service during the same hour does not
    // belong in this postmortem.
    //
    // `catalog_entities` rows hold component names; we resolve the ids
    // back to names so the filter on deployment_events can match the
    // rollout_name / namespace text.
    let window_end = inc
        .resolved_at
        .or(inc.mitigated_at)
        .unwrap_or(inc.detected_at);
    let window_start = inc.started_at;
    let deploys: Vec<DeploymentEvent> = if inc.affected_component_ids.is_empty() {
        Vec::new()
    } else {
        let component_names: Vec<String> = sqlx::query_scalar::<_, String>(
            "SELECT name FROM catalog_entities WHERE id = ANY($1)",
        )
        .bind(&inc.affected_component_ids)
        .fetch_all(pool)
        .await
        .unwrap_or_default();

        if component_names.is_empty() {
            Vec::new()
        } else {
            sqlx::query_as::<_, DeploymentEvent>(
                r#"SELECT * FROM deployment_events
                   WHERE rollout_name = ANY($1)
                     AND created_at >= $2
                     AND created_at <= $3
                   ORDER BY created_at ASC"#,
            )
            .bind(&component_names)
            .bind(window_start)
            .bind(window_end)
            .fetch_all(pool)
            .await
            .unwrap_or_default()
        }
    };

    let markdown = render_markdown(&inc, &timeline, &severity_history, &participants, &deploys);
    let action_items = Vec::new(); // Populated once IC edits the draft.

    Ok(PostmortemDraft {
        markdown,
        action_items,
    })
}

/// Render the markdown body. The shape follows Google SRE Book ch.15
/// headings so tooling that already parses postmortems keeps working.
fn render_markdown(
    inc: &Incident,
    timeline: &[IncidentTimelineEvent],
    severity_history: &[IncidentSeverityHistory],
    participants: &[IncidentParticipant],
    deploys: &[DeploymentEvent],
) -> String {
    let mut s = String::new();

    s.push_str(&format!("# Postmortem · INC-{}\n\n", inc.number));
    s.push_str(&format!("**Title:** {}\n\n", inc.title));
    s.push_str("**Status:** Draft · awaiting IC review\n\n");

    // Metadata table.
    s.push_str("| Field | Value |\n");
    s.push_str("|---|---|\n");
    s.push_str(&format!("| Severity | `{}` |\n", inc.severity));
    s.push_str(&format!(
        "| Detected at | {} |\n",
        fmt_ts(Some(inc.detected_at))
    ));
    s.push_str(&format!("| Started at | {} |\n", fmt_ts(Some(inc.started_at))));
    if let Some(t) = inc.acknowledged_at {
        s.push_str(&format!("| Acknowledged | {} |\n", fmt_ts(Some(t))));
    }
    if let Some(t) = inc.mitigated_at {
        s.push_str(&format!("| Mitigated | {} |\n", fmt_ts(Some(t))));
    }
    if let Some(t) = inc.resolved_at {
        s.push_str(&format!("| Resolved | {} |\n", fmt_ts(Some(t))));
    }
    if let Some(t) = inc.closed_at {
        s.push_str(&format!("| Closed | {} |\n", fmt_ts(Some(t))));
    }
    s.push_str(&format!(
        "| Detection source | `{}` |\n",
        inc.detection_source
    ));
    if let Some(tier) = &inc.affected_customer_tier {
        s.push_str(&format!("| Customer tier | `{tier}` |\n"));
    }
    if let Some(k) = &inc.jira_key {
        s.push_str(&format!("| Jira | `{k}` |\n"));
    }
    s.push('\n');

    // ---- Summary ----------------------------------------------------
    s.push_str("## Summary\n\n");
    if let Some(impact) = &inc.impact_summary
        && !impact.trim().is_empty()
    {
        s.push_str(impact);
        s.push_str("\n\n");
    } else {
        s.push_str("_TODO — one-paragraph summary of what happened and why it mattered._\n\n");
    }

    // ---- Impact -----------------------------------------------------
    s.push_str("## Impact\n\n");
    let ttr = inc
        .resolved_at
        .zip(Some(inc.started_at))
        .map(|(r, s)| (r - s).num_seconds().max(0))
        .unwrap_or(0);
    if ttr > 0 {
        s.push_str(&format!("- **Time to resolution:** {}\n", fmt_duration(ttr)));
    }
    if !inc.affected_component_ids.is_empty() {
        s.push_str(&format!(
            "- **Affected components:** {} service(s)\n",
            inc.affected_component_ids.len()
        ));
    }
    if let Some(tier) = &inc.affected_customer_tier {
        s.push_str(&format!("- **Customer tier impacted:** `{tier}`\n"));
    }
    s.push_str("- _TODO — quantify user impact: request failure rate, affected tenants, revenue._\n\n");

    // ---- Root Cause -------------------------------------------------
    s.push_str("## Root Cause\n\n");
    if let Some(rc) = &inc.root_cause
        && !rc.trim().is_empty()
    {
        s.push_str(rc);
        s.push_str("\n\n");
    } else {
        s.push_str("_TODO — describe the primary technical cause._\n\n");
    }
    if let Some(cat) = &inc.root_cause_category {
        s.push_str(&format!("**Category:** `{cat}`\n\n"));
    }

    // ---- Detection --------------------------------------------------
    s.push_str("## Detection\n\n");
    s.push_str(&format!(
        "- **Source:** `{}`\n- **Detected at:** {}\n",
        inc.detection_source,
        fmt_ts(Some(inc.detected_at))
    ));
    if let (Some(ack), det) = (inc.acknowledged_at, inc.detected_at) {
        s.push_str(&format!(
            "- **Time to acknowledge:** {}\n",
            fmt_duration((ack - det).num_seconds().max(0))
        ));
    }
    s.push('\n');

    // ---- Resolution -------------------------------------------------
    s.push_str("## Resolution\n\n");
    s.push_str("_TODO — what action ended the incident? Who did it?_\n\n");

    // ---- Timeline ---------------------------------------------------
    s.push_str("## Timeline\n\n");
    if timeline.is_empty() {
        s.push_str("_No timeline events recorded._\n\n");
    } else {
        for ev in timeline {
            s.push_str(&format!(
                "- **{}** · `{}` — {}\n",
                fmt_ts(Some(ev.occurred_at)),
                ev.kind,
                ev.summary
            ));
        }
        s.push('\n');
    }

    // ---- Severity History -------------------------------------------
    if !severity_history.is_empty() {
        s.push_str("### Severity history\n\n");
        for h in severity_history {
            let from = h.from_severity.as_deref().unwrap_or("-");
            let reason = h.reason.as_deref().unwrap_or("");
            s.push_str(&format!(
                "- **{}** · `{from}` → `{}`{}\n",
                fmt_ts(Some(h.changed_at)),
                h.to_severity,
                if reason.is_empty() {
                    String::new()
                } else {
                    format!(" — {reason}")
                }
            ));
        }
        s.push('\n');
    }

    // ---- Related deployments ----------------------------------------
    if !deploys.is_empty() {
        s.push_str("### Related deployments (inside incident window)\n\n");
        for d in deploys {
            s.push_str(&format!(
                "- **{}** · `{}/{}` · action `{}`\n",
                fmt_ts(Some(d.created_at)),
                d.namespace,
                d.rollout_name,
                d.action
            ));
        }
        s.push('\n');
    }

    // ---- Participants -----------------------------------------------
    if !participants.is_empty() {
        s.push_str("## Participants\n\n");
        for p in participants {
            let left = p
                .left_at
                .map(|t| format!(" (left {})", fmt_ts(Some(t))))
                .unwrap_or_default();
            s.push_str(&format!(
                "- {} as **{}** — joined {}{}\n",
                p.user_id,
                p.role,
                fmt_ts(Some(p.joined_at)),
                left
            ));
        }
        s.push('\n');
    }

    // ---- Action items -----------------------------------------------
    s.push_str("## Action Items\n\n");
    s.push_str("| Owner | Description | Due | Jira |\n");
    s.push_str("|---|---|---|---|\n");
    s.push_str("| _tbd_ | _TODO — concrete engineering follow-up_ | _tbd_ | |\n\n");

    // ---- Lessons learned -------------------------------------------
    s.push_str("## Lessons Learned\n\n");
    s.push_str("### What went well\n\n- _TODO_\n\n");
    s.push_str("### What went poorly\n\n- _TODO_\n\n");
    s.push_str("### Where we got lucky\n\n- _TODO_\n\n");

    s
}

fn fmt_ts(t: Option<DateTime<Utc>>) -> String {
    match t {
        Some(t) => t.format("%Y-%m-%d %H:%M UTC").to_string(),
        None => "-".to_string(),
    }
}

fn fmt_duration(secs: i64) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if h > 0 {
        format!("{h}h{m}m")
    } else if m > 0 {
        format!("{m}m{s}s")
    } else {
        format!("{s}s")
    }
}

/// Parse the `## Action Items` table out of a postmortem markdown body.
/// Returns the rows in document order. Empty `_tbd_` rows are dropped so
/// callers don't accidentally create placeholder Jira tickets.
pub fn parse_action_items(markdown: &str) -> Vec<ActionItem> {
    let mut items = Vec::new();
    let mut in_section = false;
    let mut past_header = false;

    for line in markdown.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("## ") {
            in_section = trimmed.eq_ignore_ascii_case("## Action Items");
            past_header = false;
            continue;
        }
        if !in_section {
            continue;
        }
        // Skip header row and separator row of the markdown table.
        if !past_header {
            if trimmed.starts_with("|---") || trimmed.starts_with("|-") {
                past_header = true;
            }
            continue;
        }
        if !trimmed.starts_with('|') {
            continue;
        }
        let cells: Vec<&str> = trimmed
            .trim_matches('|')
            .split('|')
            .map(|c| c.trim())
            .collect();
        if cells.len() < 3 {
            continue;
        }
        let owner = cell_or_none(cells[0]);
        let description = cells[1].trim();
        let due = cell_or_none(cells[2]).and_then(|s| chrono::NaiveDate::parse_from_str(&s, "%Y-%m-%d").ok());
        if description.is_empty()
            || description.eq_ignore_ascii_case("_tbd_")
            || description.to_ascii_lowercase().starts_with("_todo")
        {
            continue;
        }
        items.push(ActionItem {
            owner,
            description: description.to_string(),
            due_date: due,
        });
    }

    items
}

fn cell_or_none(c: &str) -> Option<String> {
    let t = c.trim();
    if t.is_empty()
        || t.eq_ignore_ascii_case("_tbd_")
        || t.eq_ignore_ascii_case("tbd")
        || t.eq_ignore_ascii_case("-")
    {
        None
    } else {
        Some(t.to_string())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn sample_incident() -> Incident {
        Incident {
            id: Uuid::new_v4(),
            tenant_id: None,
            number: 42,
            title: "Checkout p99 spike".to_string(),
            severity: "sev2".to_string(),
            status: "resolved".to_string(),
            commander_user_id: None,
            scribe_user_id: None,
            impact_summary: Some("40% 5xx on order-api".to_string()),
            affected_component_ids: vec![],
            affected_customer_tier: Some("tier0".to_string()),
            detection_source: "alert".to_string(),
            source_issue_id: None,
            started_at: Utc.with_ymd_and_hms(2026, 5, 1, 12, 0, 0).unwrap(),
            detected_at: Utc.with_ymd_and_hms(2026, 5, 1, 12, 1, 0).unwrap(),
            acknowledged_at: Some(Utc.with_ymd_and_hms(2026, 5, 1, 12, 3, 0).unwrap()),
            mitigated_at: Some(Utc.with_ymd_and_hms(2026, 5, 1, 12, 30, 0).unwrap()),
            resolved_at: Some(Utc.with_ymd_and_hms(2026, 5, 1, 13, 0, 0).unwrap()),
            closed_at: None,
            war_room_channel_ref: None,
            bridge_url: None,
            jira_key: Some("OPS-123".to_string()),
            postmortem_doc_ref: None,
            root_cause: None,
            root_cause_category: None,
            labels: serde_json::json!({}),
            slo_budget_burn: None,
            merged_into_id: None,
            created_at: Utc.with_ymd_and_hms(2026, 5, 1, 12, 1, 0).unwrap(),
            updated_at: Utc.with_ymd_and_hms(2026, 5, 1, 13, 0, 0).unwrap(),
        }
    }

    #[test]
    fn render_includes_all_required_sections() {
        let md = render_markdown(&sample_incident(), &[], &[], &[], &[]);
        for section in [
            "# Postmortem · INC-42",
            "## Summary",
            "## Impact",
            "## Root Cause",
            "## Detection",
            "## Resolution",
            "## Timeline",
            "## Action Items",
            "## Lessons Learned",
        ] {
            assert!(
                md.contains(section),
                "expected section `{section}` in draft, got:\n{md}"
            );
        }
    }

    #[test]
    fn render_includes_impact_and_ttr() {
        let md = render_markdown(&sample_incident(), &[], &[], &[], &[]);
        assert!(md.contains("40% 5xx on order-api"));
        // started 12:00 → resolved 13:00 → 1h
        assert!(md.contains("1h0m"), "expected 1h0m TTR, got:\n{md}");
    }

    #[test]
    fn parse_action_items_skips_placeholder_row() {
        let md = render_markdown(&sample_incident(), &[], &[], &[], &[]);
        let items = parse_action_items(&md);
        assert!(
            items.is_empty(),
            "default template has only placeholder, got: {items:?}"
        );
    }

    #[test]
    fn parse_action_items_reads_filled_rows() {
        let md = "## Action Items\n\n| Owner | Description | Due | Jira |\n|---|---|---|---|\n| alice | fix retry budget | 2026-06-01 | |\n| bob | add canary gate | | |\n";
        let items = parse_action_items(md);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].owner.as_deref(), Some("alice"));
        assert_eq!(items[0].description, "fix retry budget");
        assert!(items[0].due_date.is_some());
        assert_eq!(items[1].owner.as_deref(), Some("bob"));
        assert_eq!(items[1].description, "add canary gate");
        assert!(items[1].due_date.is_none());
    }

    #[test]
    fn parse_action_items_requires_section_header() {
        // Rows outside the Action Items section must be ignored.
        let md = "## Timeline\n| x | y | z | w |\n|---|---|---|---|\n| alice | something | 2026-01-01 | |\n";
        let items = parse_action_items(md);
        assert!(items.is_empty());
    }
}
