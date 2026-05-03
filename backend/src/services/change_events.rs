//! Global change-event stream service (W10 Joint Integration).
//!
//! Writes and reads `change_events` rows. Call sites are threaded through
//! the webhook/rollout/catalog/slo paths as fire-and-forget auditing — the
//! primary table for each subsystem (`deployment_events`, `slo_burn_events`,
//! `catalog_import_runs`, `incident_timeline_events`) still owns its own
//! truth; this stream is the cross-module join key for "what changed" Q&A.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::AppResult;
use crate::models::change_event::{ChangeEvent, QueryChangesParams};

/// Insert a row into `change_events`. On DB error we log and propagate so
/// the caller can decide — for most sites we want this to be fire-and-forget
/// but for a handler-level explicit record (like an operator-submitted
/// manual change) we let the error surface.
#[allow(clippy::too_many_arguments)]
pub async fn record(
    pool: &PgPool,
    tenant_id: Option<Uuid>,
    kind: &str,
    service_id: Option<Uuid>,
    actor: serde_json::Value,
    summary: impl Into<String>,
    payload: serde_json::Value,
    source: &str,
    correlation_id: Option<String>,
) -> AppResult<Uuid> {
    let summary = summary.into();
    let id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO change_events
               (tenant_id, kind, service_id, actor, summary, payload,
                correlation_id, source)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
           RETURNING id"#,
    )
    .bind(tenant_id)
    .bind(kind)
    .bind(service_id)
    .bind(&actor)
    .bind(&summary)
    .bind(&payload)
    .bind(&correlation_id)
    .bind(source)
    .fetch_one(pool)
    .await?;

    Ok(id)
}

/// Convenience wrapper around [`record`] that swallows errors with a
/// `tracing::warn!`. Use from webhook / alert / rollout pipelines where a
/// failed change-event write must not fail the primary operation.
#[allow(clippy::too_many_arguments)]
pub async fn record_best_effort(
    pool: &PgPool,
    tenant_id: Option<Uuid>,
    kind: &str,
    service_id: Option<Uuid>,
    actor: serde_json::Value,
    summary: impl Into<String>,
    payload: serde_json::Value,
    source: &str,
    correlation_id: Option<String>,
) {
    let summary = summary.into();
    if let Err(e) = record(
        pool,
        tenant_id,
        kind,
        service_id,
        actor,
        summary.clone(),
        payload,
        source,
        correlation_id,
    )
    .await
    {
        tracing::warn!(
            kind = %kind,
            source = %source,
            error = %e,
            "change_events: failed to record '{}'",
            summary
        );
    }
}

/// Query `change_events` with optional filters. Applies `tenant_filter`
/// unconditionally when `Some(_)` — the handler decides whether a caller is
/// allowed to bypass tenant scoping (super-admin).
pub async fn query(
    pool: &PgPool,
    params: QueryChangesParams,
    tenant_filter: Option<Uuid>,
) -> AppResult<Vec<ChangeEvent>> {
    // We build SQL piecewise because sqlx's compile-time macros don't love
    // dynamic optional filters, and the query_as runtime form keeps tenant
    // logic straightforward.
    let limit = params.limit.unwrap_or(100).clamp(1, 500);

    let mut sql = String::from("SELECT * FROM change_events WHERE 1=1");
    let mut bindings: Vec<ChangeEventBinding> = Vec::new();

    if let Some(tenant) = tenant_filter {
        bindings.push(ChangeEventBinding::Uuid(tenant));
        sql.push_str(&format!(" AND tenant_id = ${}", bindings.len()));
    }
    if let Some(svc) = params.service_id {
        bindings.push(ChangeEventBinding::Uuid(svc));
        sql.push_str(&format!(" AND service_id = ${}", bindings.len()));
    }
    if let Some(kind) = params.kind.as_deref() {
        bindings.push(ChangeEventBinding::Str(kind.to_string()));
        sql.push_str(&format!(" AND kind = ${}", bindings.len()));
    }
    if let Some(since) = params.since {
        bindings.push(ChangeEventBinding::Ts(since));
        sql.push_str(&format!(" AND occurred_at >= ${}", bindings.len()));
    }
    if let Some(until) = params.until {
        bindings.push(ChangeEventBinding::Ts(until));
        sql.push_str(&format!(" AND occurred_at <= ${}", bindings.len()));
    }

    sql.push_str(" ORDER BY occurred_at DESC");
    // SAFETY: `limit` is clamped in QueryChangesParams parsing to 1..=500
    // before reaching this builder, so interpolating it into SQL is
    // safe (no user-controlled chars, value is a bounded i64).
    sql.push_str(&format!(" LIMIT {}", limit));

    let mut q = sqlx::query_as::<_, ChangeEvent>(&sql);
    for b in &bindings {
        q = match b {
            ChangeEventBinding::Uuid(v) => q.bind(*v),
            ChangeEventBinding::Str(s) => q.bind(s.clone()),
            ChangeEventBinding::Ts(t) => q.bind(*t),
        };
    }

    let rows = q.fetch_all(pool).await?;
    Ok(rows)
}

/// Internal binding enum so we can thread heterogeneous types through
/// dynamic SQL. Bounded set — nothing user-provided lands here unescaped.
enum ChangeEventBinding {
    Uuid(Uuid),
    Str(String),
    Ts(DateTime<Utc>),
}
