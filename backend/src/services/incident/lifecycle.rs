//! Incident lifecycle orchestration — wraps `INSERT incidents` with the
//! automation pipeline that should run every time a new incident materializes:
//! timeline seed + async Slack war room + async Jira ticket.
//!
//! The pipeline is strictly non-blocking: `create_incident_with_automation`
//! returns the DB row as soon as it is written. War-room + Jira run in a
//! `tokio::spawn` task so the UI never waits on external APIs (target
//! < 300ms perceived latency per AGENT_BRIEF W3).

use chrono::Utc;
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::models::incident::{self, CreateIncidentRequest, Incident};
use crate::services::incident::{timeline, timeline_bus::TimelineBus, war_room};

/// What triggered the incident. `create_from_source` uses it to seed the
/// timeline + stamp the `detection_source` column.
#[allow(dead_code)]
pub enum IncidentSource {
    Alert { issue_id: Uuid },
    SloBurn { slo_burn_event_id: Uuid },
    Manual { user_id: Uuid },
}

impl IncidentSource {
    fn detection_source(&self) -> &'static str {
        match self {
            IncidentSource::Alert { .. } => incident::DETECTION_SOURCE_ALERT,
            IncidentSource::SloBurn { .. } => incident::DETECTION_SOURCE_SLO_BURN,
            IncidentSource::Manual { .. } => incident::DETECTION_SOURCE_MANUAL,
        }
    }

    fn actor(&self) -> serde_json::Value {
        match self {
            IncidentSource::Alert { issue_id } => serde_json::json!({
                "kind": "system",
                "source": "alert_promote",
                "issue_id": issue_id,
            }),
            IncidentSource::SloBurn { slo_burn_event_id } => serde_json::json!({
                "kind": "system",
                "source": "slo_burn_promote",
                "slo_burn_event_id": slo_burn_event_id,
            }),
            IncidentSource::Manual { user_id } => serde_json::json!({
                "kind": "user",
                "user_id": user_id,
            }),
        }
    }
}

/// Creates an incident and launches the war-room automation in the
/// background. Returns the incident record immediately; the automation
/// failure (if any) is logged but does not propagate.
pub async fn create_incident_with_automation(
    pool: &PgPool,
    bus: Arc<TimelineBus>,
    tenant_id: Option<Uuid>,
    source: IncidentSource,
    req: CreateIncidentRequest,
) -> AppResult<Incident> {
    let row = create_incident_bare(pool, tenant_id, &source, req).await?;

    // Timeline seed — after the row has landed.
    seed_timeline(pool, &bus, &row, &source).await;

    // Fire-and-forget war-room automation (Slack + Jira). Any errors are
    // logged inside the task.
    spawn_war_room_for_incident(pool.clone(), bus.clone(), row.id);

    Ok(row)
}

/// Insert a new incident row without touching the timeline or spawning
/// war-room automation. Callers that need transactional claim-then-create
/// semantics (e.g. `promote_to_incident`) use this through `*_in_tx`.
pub async fn create_incident_bare(
    pool: &PgPool,
    tenant_id: Option<Uuid>,
    source: &IncidentSource,
    req: CreateIncidentRequest,
) -> AppResult<Incident> {
    validate_create(&req)?;

    let detection_source = source.detection_source();
    let labels = req
        .labels
        .clone()
        .unwrap_or_else(|| serde_json::json!({}));
    let started_at = normalise_started_at(req.started_at);

    let row = sqlx::query_as::<_, Incident>(
        r#"INSERT INTO incidents (
               tenant_id, title, severity, status, commander_user_id, scribe_user_id,
               impact_summary, affected_component_ids, affected_customer_tier,
               detection_source, source_issue_id, started_at, detected_at, bridge_url,
               labels
           )
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, NOW(), $13, $14)
           RETURNING *"#,
    )
    .bind(tenant_id)
    .bind(req.title.trim())
    .bind(&req.severity)
    .bind(incident::STATUS_TRIGGERED)
    .bind(req.commander_user_id)
    .bind(req.scribe_user_id)
    .bind(&req.impact_summary)
    .bind(&req.affected_component_ids)
    .bind(&req.affected_customer_tier)
    .bind(detection_source)
    .bind(req.source_issue_id)
    .bind(started_at)
    .bind(&req.bridge_url)
    .bind(&labels)
    .fetch_one(pool)
    .await?;

    Ok(row)
}

/// Transaction-scoped variant of `create_incident_bare` — same INSERT, but
/// routed through a caller-owned `sqlx::Transaction`. Used by
/// `promote_to_incident` so the claim-then-create sequence is atomic.
pub async fn create_incident_bare_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant_id: Option<Uuid>,
    source: &IncidentSource,
    req: CreateIncidentRequest,
) -> AppResult<Incident> {
    validate_create(&req)?;

    let detection_source = source.detection_source();
    let labels = req
        .labels
        .clone()
        .unwrap_or_else(|| serde_json::json!({}));
    let started_at = normalise_started_at(req.started_at);

    let row = sqlx::query_as::<_, Incident>(
        r#"INSERT INTO incidents (
               tenant_id, title, severity, status, commander_user_id, scribe_user_id,
               impact_summary, affected_component_ids, affected_customer_tier,
               detection_source, source_issue_id, started_at, detected_at, bridge_url,
               labels
           )
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, NOW(), $13, $14)
           RETURNING *"#,
    )
    .bind(tenant_id)
    .bind(req.title.trim())
    .bind(&req.severity)
    .bind(incident::STATUS_TRIGGERED)
    .bind(req.commander_user_id)
    .bind(req.scribe_user_id)
    .bind(&req.impact_summary)
    .bind(&req.affected_component_ids)
    .bind(&req.affected_customer_tier)
    .bind(detection_source)
    .bind(req.source_issue_id)
    .bind(started_at)
    .bind(&req.bridge_url)
    .bind(&labels)
    .fetch_one(&mut **tx)
    .await?;

    Ok(row)
}

/// P2 #21: detected_at is always NOW() (DB-side), but `started_at` comes
/// from the client. If a caller accidentally submits a future timestamp
/// (clock skew, malformed sync, test fixture) we'd end up with an
/// incident whose incident-began-at time is in the future — which
/// inverts downstream MTTR/MTTA maths. Clamp to `now` and warn so the
/// caller sees it in logs. Past timestamps are left untouched (a real
/// user might promote a silent incident hours after it started).
fn normalise_started_at(raw: chrono::DateTime<Utc>) -> chrono::DateTime<Utc> {
    let now = Utc::now();
    if raw > now {
        tracing::warn!(
            "incident started_at={raw} is in the future; clamping to now={now}"
        );
        now
    } else {
        raw
    }
}

/// Seed a timeline event for a freshly created incident. Errors are logged
/// but not returned — the incident row is already authoritative.
pub async fn seed_timeline(
    pool: &PgPool,
    bus: &TimelineBus,
    row: &Incident,
    source: &IncidentSource,
) {
    let actor = source.actor();
    if let Err(e) = timeline::record_event(
        pool,
        bus,
        row.id,
        timeline::KIND_STATUS_CHANGED,
        actor,
        "Incident created",
        serde_json::json!({
            "from": null,
            "to": row.status,
            "detection_source": row.detection_source,
            "source_issue_id": row.source_issue_id,
        }),
    )
    .await
    {
        tracing::warn!("timeline seed failed for incident {}: {}", row.id, e);
    }
}

/// Spawn the background war-room automation (Slack channel + Jira ticket)
/// for the given incident. Non-blocking; returns immediately.
pub fn spawn_war_room_for_incident(pool: PgPool, bus: Arc<TimelineBus>, incident_id: Uuid) {
    tokio::spawn(async move {
        let result = war_room::spawn_war_room(&pool, bus, incident_id).await;
        if !result.errors.is_empty() {
            tracing::warn!(
                "war_room automation produced {} warning(s) for incident {}: {:?}",
                result.errors.len(),
                incident_id,
                result.errors
            );
        }
    });
}

fn validate_create(req: &CreateIncidentRequest) -> AppResult<()> {
    if req.title.trim().is_empty() {
        return Err(AppError::BadRequest("title is required".to_string()));
    }
    if !Incident::is_valid_severity(&req.severity) {
        return Err(AppError::BadRequest(format!(
            "invalid severity: {}",
            req.severity
        )));
    }
    Ok(())
}
