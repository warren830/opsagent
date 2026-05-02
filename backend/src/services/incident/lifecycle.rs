//! Incident lifecycle orchestration — wraps `INSERT incidents` with the
//! automation pipeline that should run every time a new incident materializes:
//! timeline seed + async Slack war room + async Jira ticket.
//!
//! The pipeline is strictly non-blocking: `create_incident_with_automation`
//! returns the DB row as soon as it is written. War-room + Jira run in a
//! `tokio::spawn` task so the UI never waits on external APIs (target
//! < 300ms perceived latency per AGENT_BRIEF W3).

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
    // ---- 1. Basic validation -------------------------------------------------
    if req.title.trim().is_empty() {
        return Err(AppError::BadRequest("title is required".to_string()));
    }
    if !Incident::is_valid_severity(&req.severity) {
        return Err(AppError::BadRequest(format!(
            "invalid severity: {}",
            req.severity
        )));
    }

    let detection_source = source.detection_source();
    let labels = req
        .labels
        .clone()
        .unwrap_or_else(|| serde_json::json!({}));

    // ---- 2. INSERT incidents -------------------------------------------------
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
    .bind(req.started_at)
    .bind(&req.bridge_url)
    .bind(&labels)
    .fetch_one(pool)
    .await?;

    // ---- 3. Timeline seed ----------------------------------------------------
    let actor = source.actor();
    if let Err(e) = timeline::record_event(
        pool,
        &bus,
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

    // ---- 4. Background automation -------------------------------------------
    // Fire-and-forget: slack + jira happen after the handler has already
    // responded. Any error is logged inside `spawn_war_room`.
    //
    // TODO(W6+): auto-spawn an agent session here with
    // `context_type='incident'` + `context_id=row.id` so the copilot is
    // alive the moment the war room opens. Blocked on a backend-only
    // entry point to `ClaudeService` — today only `POST /api/chat` can
    // mint a new Claude CLI session, and that path is tied to the
    // caller's SSE stream (user-facing). Options to close the gap:
    //   1. Extract a headless `ClaudeService::spawn_session` that runs
    //      Claude CLI without piping back to an HTTP stream, store the
    //      generated session id on `claude_sessions` with the incident
    //      context linkage, and post the "Agent online" card to the
    //      war-room channel.
    //   2. Queue a seed message for the first human visitor so the chat
    //      composer loads the template prompt pre-filled.
    // For MVP the IC opens the war-room page and the AGENT CHAT panel
    // spawns the session on first message, which is good enough and
    // keeps the lifecycle non-blocking.
    //
    // TODO(W6+): historical-similar-incidents vector retrieval. Needs a
    // pgvector column on `knowledge_files` (or a dedicated embeddings
    // table) plus an embedding service call. Not in MVP scope.
    let pool_bg = pool.clone();
    let bus_bg = bus.clone();
    let incident_id = row.id;
    tokio::spawn(async move {
        let result = war_room::spawn_war_room(&pool_bg, bus_bg, incident_id).await;
        if !result.errors.is_empty() {
            tracing::warn!(
                "war_room automation produced {} warning(s) for incident {}: {:?}",
                result.errors.len(),
                incident_id,
                result.errors
            );
        }
    });

    Ok(row)
}
