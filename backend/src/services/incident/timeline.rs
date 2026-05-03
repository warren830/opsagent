//! Timeline event recorder — a thin helper around `incident_timeline_events`.
//!
//! All incident mutations (status transitions, severity changes, participant
//! add/remove, deployments, chat activity) land here so there is a single
//! SQL statement shared across handlers. See
//! `docs/platform-evolution.md` §5.4.
//!
//! W4: every successful insert also publishes onto the in-process
//! `TimelineBus` so SSE subscribers on
//! `/api/incidents/:id/timeline/stream` see events in real time without
//! polling.

use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::models::incident::IncidentTimelineEvent;
use crate::services::incident::timeline_bus::{TimelineBroadcast, TimelineBus};

// ---------------------------------------------------------------------------
// Canonical timeline `kind` values. Free-form strings are allowed on the
// column, but handlers should prefer these constants to keep the UI filter
// set stable.
// ---------------------------------------------------------------------------
pub const KIND_STATUS_CHANGED: &str = "incident_status_changed";
pub const KIND_SEVERITY_CHANGED: &str = "incident_severity_changed";
pub const KIND_PARTICIPANT_JOIN: &str = "join";
pub const KIND_PARTICIPANT_LEAVE: &str = "leave";
pub const KIND_UPDATE_PUBLISHED: &str = "update_published";
pub const KIND_DEPLOYMENT: &str = "deployment";
pub const KIND_CHAT_TOOL_CALL: &str = "chat_tool_call";
// W4 additions — emitted by argocd_webhook / rollout handlers.
pub const KIND_DEPLOY_STARTED: &str = "deploy_started";
pub const KIND_DEPLOY_SUCCEEDED: &str = "deploy_succeeded";
pub const KIND_DEPLOY_FAILED: &str = "deploy_failed";
pub const KIND_ROLLBACK_INITIATED: &str = "rollback_initiated";
pub const KIND_PROMOTE_INITIATED: &str = "promote_initiated";

/// Inserts a single row into `incident_timeline_events` and fans it out onto
/// the `TimelineBus` for live SSE subscribers.
///
/// `actor` is a JSONB blob describing who caused the event — typical shapes:
///
/// - `{"kind":"user","user_id":"…","username":"…"}`
/// - `{"kind":"system","source":"state_machine"}`
/// - `{"kind":"agent","agent":"claude","session_id":"…"}`
///
/// `payload` is an event-specific JSONB blob (empty `{}` if unused).
///
/// The DB insert is authoritative — if it fails we return the error. The
/// bus publish is fire-and-forget; a saturated channel or zero subscribers
/// never surface as an error.
#[allow(clippy::too_many_arguments)]
pub async fn record_event(
    pool: &PgPool,
    bus: &TimelineBus,
    incident_id: Uuid,
    kind: &str,
    actor: serde_json::Value,
    summary: &str,
    payload: serde_json::Value,
) -> AppResult<()> {
    let row = sqlx::query_as::<_, IncidentTimelineEvent>(
        r#"INSERT INTO incident_timeline_events
               (incident_id, kind, actor, summary, payload)
           VALUES ($1, $2, $3, $4, $5)
           RETURNING *"#,
    )
    .bind(incident_id)
    .bind(kind)
    .bind(&actor)
    .bind(summary)
    .bind(&payload)
    .fetch_one(pool)
    .await
    .map_err(AppError::from)?;

    bus.publish(TimelineBroadcast {
        incident_id,
        event: row,
    });
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

/// Convenience: build an `actor` JSON blob for a system/automation event.
#[allow(dead_code)]
pub fn system_actor(source: &str) -> serde_json::Value {
    serde_json::json!({
        "kind": "system",
        "source": source,
    })
}

/// Convenience: build an `actor` JSON blob for an AI-agent event (chat
/// tool-call forwarded into the incident timeline).
pub fn agent_actor(agent: &str, session_id: &str) -> serde_json::Value {
    serde_json::json!({
        "kind": "agent",
        "agent": agent,
        "session_id": session_id,
    })
}

/// Match a deployment-ish event (`component_name` being the ArgoCD app name,
/// rollout name, or k8s deployment name) against every non-closed incident
/// and write one timeline row per match.
///
/// Matching strategy (MVP, per AGENT_BRIEF W4):
///
/// 1. Resolve `component_name` to any number of `catalog_entities.id` rows
///    (same tenant, any kind — we don't enforce `kind='component'` because
///    catalog kinds aren't normalized yet and we want the widest possible
///    matcher).
/// 2. For each incident whose `status <> 'closed'` and whose
///    `affected_component_ids` array overlaps those component ids, record
///    the event.
///
/// Deliberately fire-and-forget at the outer layer: failures log at WARN,
/// the caller (webhook / rollout promote) does not branch on the outcome.
pub async fn fanout_deploy_event_to_incidents(
    pool: &PgPool,
    bus: &TimelineBus,
    tenant_id: Option<Uuid>,
    component_name: &str,
    kind: &str,
    actor: serde_json::Value,
    summary: &str,
    payload: serde_json::Value,
) {
    // Two tenants can both have a catalog entity named, say, `checkout`.
    // Without the tenant filter, a deploy event for tenant A's `checkout`
    // resolves to both tenants' entity ids, which then overlaps an open
    // incident in tenant B that happens to affect its own `checkout`. The
    // result: tenant B's war room sees a timeline entry about tenant A's
    // deploy, leaking both the existence of that deploy and whatever
    // summary/payload the caller passed. Require callers to provide the
    // tenant context so we can filter the catalog lookup at the source.
    let Some(tenant_id) = tenant_id else {
        tracing::debug!(
            "timeline fanout: no tenant context for component '{}', skipping",
            component_name
        );
        return;
    };

    let component_ids: Vec<Uuid> = match sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM catalog_entities WHERE name = $1 AND tenant_id = $2",
    )
    .bind(component_name)
    .bind(tenant_id)
    .fetch_all(pool)
    .await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(
                "timeline fanout: lookup for component '{}' failed: {}",
                component_name,
                e
            );
            return;
        }
    };

    if component_ids.is_empty() {
        tracing::debug!(
            "timeline fanout: no catalog_entities match '{}' in tenant {}, skipping",
            component_name,
            tenant_id
        );
        return;
    }

    // Find every non-closed incident whose affected_component_ids overlaps
    // our match set (`&&` is PG array overlap). Tenant scoping is
    // redundant given the catalog_ids are already tenant-scoped, but we
    // add the explicit `tenant_id = $2` predicate as a defence-in-depth
    // guard against future bugs where a stray cross-tenant id sneaks
    // into the array.
    let incident_ids: Vec<Uuid> = match sqlx::query_scalar::<_, Uuid>(
        r#"SELECT id FROM incidents
           WHERE status <> 'closed'
             AND tenant_id = $2
             AND affected_component_ids && $1"#,
    )
    .bind(&component_ids)
    .bind(tenant_id)
    .fetch_all(pool)
    .await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(
                "timeline fanout: active incident lookup failed for '{}': {}",
                component_name,
                e
            );
            return;
        }
    };

    if incident_ids.is_empty() {
        return;
    }

    tracing::info!(
        "timeline fanout: {} active incident(s) match component '{}', kind='{}'",
        incident_ids.len(),
        component_name,
        kind
    );

    for inc_id in incident_ids {
        if let Err(e) = record_event(
            pool,
            bus,
            inc_id,
            kind,
            actor.clone(),
            summary,
            payload.clone(),
        )
        .await
        {
            tracing::warn!(
                "timeline fanout: write failed for incident {}: {}",
                inc_id,
                e
            );
        }
    }
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

    #[test]
    fn system_actor_shape_is_stable() {
        let v = system_actor("state_machine");
        assert_eq!(v["kind"], "system");
        assert_eq!(v["source"], "state_machine");
    }

    #[test]
    fn agent_actor_shape_is_stable() {
        let v = agent_actor("claude", "sess-123");
        assert_eq!(v["kind"], "agent");
        assert_eq!(v["agent"], "claude");
        assert_eq!(v["session_id"], "sess-123");
    }
}
