//! Thin wrapper over the Slack Web API for war-room automation.
//!
//! Slack credentials live in the `channels` table under `platform='slack'`:
//!
//! ```json
//! { "bot_token": "xoxb-..." }
//! ```
//!
//! Only the subset the war-room needs is implemented here:
//!
//! - `get_slack_token` — pull the bot token for a tenant's first enabled
//!   Slack channel. Returns `None` if the tenant has no Slack integration
//!   configured (war-room then falls back to a stub path).
//! - `slack_create_channel` / `slack_post_message` / `slack_invite_users` /
//!   `slack_lookup_user_by_email` — direct calls to `slack.com/api/*`.
//!
//! Every call returns `AppResult<_>` — transport errors bubble up as
//! `AppError::HttpClient`, logical errors (non-`ok` response) do the same
//! with the `error` field from Slack.

use serde::Deserialize;
use sqlx::PgPool;
use std::sync::OnceLock;
use std::time::Duration;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::models::channel::Channel;

const SLACK_API_BASE: &str = "https://slack.com/api";

/// Process-wide `reqwest::Client` for Slack Web API calls. Built once with
/// the same timeout/pool defaults as the main HTTP client in `main.rs` so
/// a hung Slack API call can't dangle the war-room task forever. P1 #6.
static SLACK_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

fn slack_http_client() -> &'static reqwest::Client {
    SLACK_CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .connect_timeout(Duration::from_secs(5))
            .pool_max_idle_per_host(10)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new())
    })
}

// ---------------------------------------------------------------------------
// Token lookup.
// ---------------------------------------------------------------------------

/// Find a Slack bot token for the given tenant. Picks the first enabled
/// Slack channel row scoped to that tenant. Returns `Ok(None)` when no
/// Slack integration exists for the tenant — callers should degrade
/// gracefully.
///
/// **Strict tenant match** — a `None` tenant no longer falls back to the
/// first globally-enabled Slack row. That fallback let a background job
/// (or a tenant-less internal caller) leak a private bot token into a
/// different tenant's war room. Callers that truly need a shared bot
/// must thread the right tenant id through explicitly.
pub async fn get_slack_token(pool: &PgPool, tenant_id: Option<Uuid>) -> AppResult<Option<String>> {
    let Some(tenant_id) = tenant_id else {
        return Ok(None);
    };
    let row = sqlx::query_as::<_, Channel>(
        "SELECT * FROM channels
         WHERE platform = 'slack' AND enabled = true
           AND tenant_id = $1
         ORDER BY created_at ASC
         LIMIT 1",
    )
    .bind(tenant_id)
    .fetch_optional(pool)
    .await?;

    let Some(channel) = row else {
        return Ok(None);
    };

    let token = channel
        .credentials
        .get("bot_token")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    Ok(token)
}

// ---------------------------------------------------------------------------
// Slack API responses.
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct SlackBaseResponse {
    ok: bool,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SlackChannelResponse {
    ok: bool,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    channel: Option<SlackChannel>,
}

#[derive(Debug, Deserialize)]
struct SlackChannel {
    id: String,
    #[serde(default)]
    #[allow(dead_code)]
    name: String,
}

#[derive(Debug, Deserialize)]
struct SlackUserResponse {
    ok: bool,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    user: Option<SlackUser>,
}

#[derive(Debug, Deserialize)]
struct SlackUser {
    id: String,
}

/// The result of [`slack_create_channel`].
pub struct CreatedChannel {
    pub id: String,
    pub name: String,
}

// ---------------------------------------------------------------------------
// API calls.
// ---------------------------------------------------------------------------

/// Create a public Slack channel. If the channel already exists (because
/// the incident is being re-promoted) Slack returns `name_taken` — callers
/// should treat that as success and look up the existing channel.
pub async fn slack_create_channel(token: &str, name: &str) -> AppResult<CreatedChannel> {
    let http = slack_http_client();
    let resp = http
        .post(format!("{SLACK_API_BASE}/conversations.create"))
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json; charset=utf-8")
        .json(&serde_json::json!({
            "name": name,
            "is_private": false,
        }))
        .send()
        .await
        .map_err(|e| AppError::HttpClient(format!("slack conversations.create: {e}")))?;

    let body: SlackChannelResponse = resp
        .json()
        .await
        .map_err(|e| AppError::HttpClient(format!("slack parse create: {e}")))?;

    if !body.ok {
        return Err(AppError::HttpClient(format!(
            "slack conversations.create failed: {}",
            body.error.unwrap_or_else(|| "unknown".to_string())
        )));
    }

    let channel = body
        .channel
        .ok_or_else(|| AppError::HttpClient("slack create returned no channel".into()))?;
    Ok(CreatedChannel {
        id: channel.id,
        name: name.to_string(),
    })
}

/// Post a plain-text message to a channel (block kit is out of scope for
/// MVP). Returns `Ok(())` if Slack accepts the message.
pub async fn slack_post_message(token: &str, channel_id: &str, text: &str) -> AppResult<()> {
    let http = slack_http_client();
    let resp = http
        .post(format!("{SLACK_API_BASE}/chat.postMessage"))
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json; charset=utf-8")
        .json(&serde_json::json!({
            "channel": channel_id,
            "text": text,
            "unfurl_links": false,
        }))
        .send()
        .await
        .map_err(|e| AppError::HttpClient(format!("slack chat.postMessage: {e}")))?;

    let body: SlackBaseResponse = resp
        .json()
        .await
        .map_err(|e| AppError::HttpClient(format!("slack parse post: {e}")))?;

    if !body.ok {
        return Err(AppError::HttpClient(format!(
            "slack chat.postMessage failed: {}",
            body.error.unwrap_or_else(|| "unknown".to_string())
        )));
    }
    Ok(())
}

/// Invite a batch of Slack user IDs into a channel. Slack's invite endpoint
/// accepts up to ~30 users per call; we just pass the comma-joined list.
pub async fn slack_invite_users(
    token: &str,
    channel_id: &str,
    user_ids: &[String],
) -> AppResult<()> {
    if user_ids.is_empty() {
        return Ok(());
    }
    let http = slack_http_client();
    let resp = http
        .post(format!("{SLACK_API_BASE}/conversations.invite"))
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json; charset=utf-8")
        .json(&serde_json::json!({
            "channel": channel_id,
            "users": user_ids.join(","),
        }))
        .send()
        .await
        .map_err(|e| AppError::HttpClient(format!("slack conversations.invite: {e}")))?;

    let body: SlackBaseResponse = resp
        .json()
        .await
        .map_err(|e| AppError::HttpClient(format!("slack parse invite: {e}")))?;

    if !body.ok {
        // `already_in_channel` is benign — callers treat it as success.
        let err = body.error.unwrap_or_else(|| "unknown".to_string());
        if err == "already_in_channel" {
            return Ok(());
        }
        return Err(AppError::HttpClient(format!(
            "slack conversations.invite failed: {err}"
        )));
    }
    Ok(())
}

/// Look up a Slack user by email. Returns `Ok(None)` if Slack cannot find
/// a match (`users_not_found`), which is common for external stakeholders.
pub async fn slack_lookup_user_by_email(
    token: &str,
    email: &str,
) -> AppResult<Option<String>> {
    let http = slack_http_client();
    let resp = http
        .get(format!("{SLACK_API_BASE}/users.lookupByEmail"))
        .header("Authorization", format!("Bearer {token}"))
        .query(&[("email", email)])
        .send()
        .await
        .map_err(|e| AppError::HttpClient(format!("slack users.lookupByEmail: {e}")))?;

    let body: SlackUserResponse = resp
        .json()
        .await
        .map_err(|e| AppError::HttpClient(format!("slack parse lookup: {e}")))?;

    if !body.ok {
        let err = body.error.unwrap_or_else(|| "unknown".to_string());
        if err == "users_not_found" {
            return Ok(None);
        }
        return Err(AppError::HttpClient(format!(
            "slack users.lookupByEmail failed: {err}"
        )));
    }
    Ok(body.user.map(|u| u.id))
}

// ---------------------------------------------------------------------------
// Tests.
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    /// `#inc-YYYYMMDD-<kebab>` names must survive Slack's 80-char limit
    /// and `[a-z0-9._-]+` character set. We take kebab_title from
    /// `war_room::build_channel_name`, which is tested separately.
    #[test]
    fn slack_base_url_is_https() {
        assert!(SLACK_API_BASE.starts_with("https://"));
    }
}
