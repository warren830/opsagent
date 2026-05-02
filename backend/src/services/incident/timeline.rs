//! Timeline event recorder — a thin helper around `incident_timeline_events`.
//!
//! All incident mutations (status transitions, severity changes, participant
//! add/remove, deployments, chat activity) land here so there is a single
//! SQL statement shared across handlers. See
//! `docs/platform-evolution.md` §5.4.

use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{AppError, AppResult};

/// Canonical timeline `kind` values. Free-form strings are allowed on the
/// column, but handlers should prefer these constants to keep the UI filter
/// set stable.
pub const KIND_STATUS_CHANGED: &str = "incident_status_changed";
pub const KIND_SEVERITY_CHANGED: &str = "incident_severity_changed";
pub const KIND_PARTICIPANT_JOIN: &str = "join";
pub const KIND_PARTICIPANT_LEAVE: &str = "leave";
pub const KIND_UPDATE_PUBLISHED: &str = "update_published";
pub const KIND_DEPLOYMENT: &str = "deployment";
pub const KIND_CHAT_TOOL_CALL: &str = "chat_tool_call";

/// Inserts a single row into `incident_timeline_events`.
///
/// `actor` is a JSONB blob describing who caused the event — typical shapes:
///
/// - `{"kind":"user","user_id":"…","username":"…"}`
/// - `{"kind":"system","source":"state_machine"}`
/// - `{"kind":"agent","agent":"claude","session_id":"…"}`
///
/// `payload` is an event-specific JSONB blob (empty `{}` if unused).
#[allow(clippy::too_many_arguments)]
pub async fn record_event(
    pool: &PgPool,
    incident_id: Uuid,
    kind: &str,
    actor: serde_json::Value,
    summary: &str,
    payload: serde_json::Value,
) -> AppResult<()> {
    sqlx::query(
        r#"INSERT INTO incident_timeline_events
               (incident_id, kind, actor, summary, payload)
           VALUES ($1, $2, $3, $4, $5)"#,
    )
    .bind(incident_id)
    .bind(kind)
    .bind(&actor)
    .bind(summary)
    .bind(&payload)
    .execute(pool)
    .await
    .map_err(AppError::from)?;
    Ok(())
}

/// Convenience: build an `actor` JSON blob for a user-triggered event.
pub fn user_actor(user_id: Uuid, username: &str) -> serde_json::Value {
    serde_json::json!({
        "kind": "user",
        "user_id": user_id,
        "username": username,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn user_actor_shape_is_stable() {
        let uid = Uuid::new_v4();
        let v = user_actor(uid, "alice");
        assert_eq!(v["kind"], "user");
        assert_eq!(v["username"], "alice");
        assert_eq!(v["user_id"], serde_json::Value::String(uid.to_string()));
    }
}
