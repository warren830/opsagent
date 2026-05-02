use axum::{
    Json,
    extract::{Path, Query, State},
    response::sse::{Event, Sse},
};
use std::convert::Infallible;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio_stream::Stream;
use uuid::Uuid;

use crate::AppState;
use crate::error::{AppError, AppResult};
use crate::middleware::auth::AuthUser;
use crate::models::incident::{CreateIncidentRequest, Incident};
use crate::models::issue::{Issue, IssueListQuery, PromoteRequest, UpdateIssueRequest};
use crate::services::claude::StreamChunk;
use crate::services::incident::lifecycle::{self, IncidentSource};

type SseEventStream = Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>>;

/// GET /api/issues
pub async fn list(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Query(query): Query<IssueListQuery>,
) -> AppResult<Json<Vec<Issue>>> {
    let rows = if auth_user.is_super_admin() {
        sqlx::query_as::<_, Issue>(
            r#"SELECT * FROM issues
               WHERE ($1::TEXT IS NULL OR status = $1)
                 AND ($2::TEXT IS NULL OR severity = $2)
                 AND ($3::TEXT IS NULL OR issue_type = $3)
               ORDER BY created_at DESC"#,
        )
        .bind(&query.status)
        .bind(&query.severity)
        .bind(&query.issue_type)
        .fetch_all(&state.pool)
        .await?
    } else {
        sqlx::query_as::<_, Issue>(
            r#"SELECT * FROM issues
               WHERE tenant_id = $1
                 AND ($2::TEXT IS NULL OR status = $2)
                 AND ($3::TEXT IS NULL OR severity = $3)
                 AND ($4::TEXT IS NULL OR issue_type = $4)
               ORDER BY created_at DESC"#,
        )
        .bind(auth_user.tenant_id)
        .bind(&query.status)
        .bind(&query.severity)
        .bind(&query.issue_type)
        .fetch_all(&state.pool)
        .await?
    };
    Ok(Json(rows))
}

/// GET /api/issues/count — count of unresolved issues (for sidebar badge)
pub async fn count(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
) -> AppResult<Json<serde_json::Value>> {
    let count: (i64,) = if auth_user.is_super_admin() {
        sqlx::query_as("SELECT COUNT(*) FROM issues WHERE status != 'resolved'")
            .fetch_one(&state.pool)
            .await?
    } else {
        sqlx::query_as("SELECT COUNT(*) FROM issues WHERE tenant_id = $1 AND status != 'resolved'")
            .bind(auth_user.tenant_id)
            .fetch_one(&state.pool)
            .await?
    };
    Ok(Json(serde_json::json!({ "count": count.0 })))
}

/// GET /api/issues/:id
pub async fn get(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Issue>> {
    let row = sqlx::query_as::<_, Issue>("SELECT * FROM issues WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Issue not found".to_string()))?;

    if !auth_user.is_super_admin() && row.tenant_id != auth_user.tenant_id {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }

    Ok(Json(row))
}

/// PUT /api/issues/:id
pub async fn update(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateIssueRequest>,
) -> AppResult<Json<Issue>> {
    if !auth_user.is_super_admin() {
        let existing = sqlx::query_as::<_, Issue>("SELECT * FROM issues WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Issue not found".to_string()))?;
        if existing.tenant_id != auth_user.tenant_id {
            return Err(AppError::Forbidden("Access denied".to_string()));
        }
    }

    let row = sqlx::query_as::<_, Issue>(
        r#"UPDATE issues SET
           title = COALESCE($2, title),
           description = COALESCE($3, description),
           severity = COALESCE($4, severity),
           status = COALESCE($5, status),
           updated_at = NOW()
           WHERE id = $1
           RETURNING *"#,
    )
    .bind(id)
    .bind(&req.title)
    .bind(&req.description)
    .bind(&req.severity)
    .bind(&req.status)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Issue not found".to_string()))?;

    Ok(Json(row))
}

/// POST /api/issues/:id/rca — SSE streaming RCA analysis
pub async fn start_rca(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Sse<axum::response::sse::KeepAliveStream<SseEventStream>> {
    // Auth check
    let issue = match fetch_and_check_issue(&state, &auth_user, id).await {
        Ok(issue) => issue,
        Err(e) => {
            let error_stream = futures::stream::once(async move {
                let chunk = StreamChunk::Error { message: e.to_string() };
                let data = serde_json::to_string(&chunk).unwrap_or_default();
                Ok::<_, Infallible>(Event::default().data(data))
            });
            return Sse::new(Box::pin(error_stream) as SseEventStream).keep_alive(
                axum::response::sse::KeepAlive::new()
                    .interval(Duration::from_secs(15))
                    .text("ping"),
            );
        }
    };

    // Check if RCA is already running — subscribe to existing stream
    if let Some(rx) = state.rca_registry.subscribe(id).await {
        let sse_stream = tokio_stream::wrappers::BroadcastStream::new(rx);
        let event_stream = tokio_stream::StreamExt::map(sse_stream, |result| {
            let data = match result {
                Ok(chunk) => serde_json::to_string(&chunk).unwrap_or_default(),
                Err(_) => serde_json::to_string(&StreamChunk::Error {
                    message: "Stream lagged".to_string(),
                })
                .unwrap_or_default(),
            };
            Ok::<_, Infallible>(Event::default().data(data))
        });
        return Sse::new(Box::pin(event_stream) as SseEventStream).keep_alive(
            axum::response::sse::KeepAlive::new()
                .interval(Duration::from_secs(15))
                .text("ping"),
        );
    }

    // Start new RCA — get a receiver before spawning
    let rx = {
        // Subscribe first, then spawn so we don't miss early chunks
        let rx_opt = state.rca_registry.subscribe(id).await;
        if let Some(rx) = rx_opt {
            rx
        } else {
            // Not yet registered — we need to trigger run_rca
            let pool = state.pool.clone();
            let config = Arc::new(state.config.clone());
            let registry = state.rca_registry.clone();

            // Pre-register so we can subscribe immediately
            // run_rca will use the existing channel
            let issue_clone = issue.clone();
            let registry_clone = registry.clone();
            let pool_clone = pool.clone();
            let config_clone = config.clone();

            // We'll subscribe after run_rca registers
            tokio::spawn(async move {
                crate::services::rca::run_rca(pool_clone, config_clone, registry_clone, issue_clone).await;
            });

            // Give a tiny moment for registration, then subscribe
            tokio::time::sleep(Duration::from_millis(50)).await;
            match state.rca_registry.subscribe(id).await {
                Some(rx) => rx,
                None => {
                    // Fallback: RCA finished instantly or failed to start
                    let error_stream = futures::stream::once(async {
                        let chunk = StreamChunk::Error {
                            message: "RCA failed to start".to_string(),
                        };
                        let data = serde_json::to_string(&chunk).unwrap_or_default();
                        Ok::<_, Infallible>(Event::default().data(data))
                    });
                    return Sse::new(Box::pin(error_stream) as SseEventStream).keep_alive(
                        axum::response::sse::KeepAlive::new()
                            .interval(Duration::from_secs(15))
                            .text("ping"),
                    );
                }
            }
        }
    };

    let sse_stream = tokio_stream::wrappers::BroadcastStream::new(rx);
    let event_stream = tokio_stream::StreamExt::map(sse_stream, |result| {
        let data = match result {
            Ok(chunk) => serde_json::to_string(&chunk).unwrap_or_default(),
            Err(_) => serde_json::to_string(&StreamChunk::Error {
                message: "Stream lagged".to_string(),
            })
            .unwrap_or_default(),
        };
        Ok::<_, Infallible>(Event::default().data(data))
    });

    Sse::new(Box::pin(event_stream) as SseEventStream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("ping"),
    )
}

/// GET /api/issues/:id/rca/status — check if RCA is currently running
pub async fn rca_status(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    // Verify access
    let _ = fetch_and_check_issue(&state, &auth_user, id).await?;
    let running = state.rca_registry.is_running(id).await;
    Ok(Json(serde_json::json!({ "running": running })))
}

/// Shared helper: fetch issue and verify tenant access
async fn fetch_and_check_issue(state: &AppState, auth_user: &AuthUser, id: Uuid) -> Result<Issue, AppError> {
    let row = sqlx::query_as::<_, Issue>("SELECT * FROM issues WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::NotFound("Issue not found".to_string()))?;
    if !auth_user.is_super_admin() && row.tenant_id != auth_user.tenant_id {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }
    Ok(row)
}

/// POST /api/issues/:id/promote — turn an issue into an incident.
///
/// Idempotent: if `issues.incident_id` is already set, returns the existing
/// incident row. Otherwise creates the incident (via
/// `lifecycle::create_incident_with_automation`, which also kicks off the
/// Slack war-room + Jira ticket in the background), writes the id back
/// onto the source issue, and returns the new incident immediately.
pub async fn promote_to_incident(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(issue_id): Path<Uuid>,
    Json(req): Json<PromoteRequest>,
) -> AppResult<Json<Incident>> {
    let issue = fetch_and_check_issue(&state, &auth_user, issue_id).await?;

    // Idempotency: issue already linked to an incident.
    if let Some(existing_inc_id) = issue.incident_id {
        let existing = sqlx::query_as::<_, Incident>("SELECT * FROM incidents WHERE id = $1")
            .bind(existing_inc_id)
            .fetch_optional(&state.pool)
            .await?
            .ok_or_else(|| {
                AppError::Internal(format!(
                    "issue {issue_id} references missing incident {existing_inc_id}"
                ))
            })?;
        return Ok(Json(existing));
    }

    if !Incident::is_valid_severity(&req.severity) {
        return Err(AppError::BadRequest(format!(
            "invalid severity: {}",
            req.severity
        )));
    }

    // Build CreateIncidentRequest from the issue + overrides.
    let affected_component_ids = req
        .affected_component_ids
        .clone()
        .unwrap_or_else(|| issue.affected_component_ids.clone());
    let impact_summary = req
        .impact_summary
        .clone()
        .or_else(|| issue.description.clone());
    let title = req
        .title
        .clone()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| issue.title.clone());

    let create_req = CreateIncidentRequest {
        title,
        severity: req.severity.clone(),
        status: None,
        detection_source: crate::models::incident::DETECTION_SOURCE_ALERT.to_string(),
        impact_summary,
        affected_component_ids,
        affected_customer_tier: None,
        source_issue_id: Some(issue.id),
        started_at: issue.created_at,
        commander_user_id: req.commander_user_id,
        scribe_user_id: None,
        bridge_url: None,
        labels: req.labels.clone(),
        tenant_id: issue.tenant_id,
    };

    let incident = lifecycle::create_incident_with_automation(
        &state.pool,
        issue.tenant_id,
        IncidentSource::Alert { issue_id: issue.id },
        create_req,
    )
    .await?;

    // Persist the back-reference so subsequent promote calls are idempotent.
    if let Err(e) = sqlx::query(
        "UPDATE issues SET incident_id = $1, updated_at = NOW() WHERE id = $2",
    )
    .bind(incident.id)
    .bind(issue.id)
    .execute(&state.pool)
    .await
    {
        tracing::warn!(
            "failed to back-ref issue {} -> incident {}: {}",
            issue.id,
            incident.id,
            e
        );
    }

    Ok(Json(incident))
}
