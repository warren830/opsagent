use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;
use uuid::Uuid;

use crate::AppState;
use crate::error::AppResult;
use crate::middleware::auth::AuthUser;
use crate::models::scheduled_job::{CreateScheduledJobRequest, JobRun, ScheduledJob, UpdateScheduledJobRequest};
use crate::services;

/// GET /api/scheduled-jobs
pub async fn list(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
) -> AppResult<Json<Vec<ScheduledJob>>> {
    let rows = services::scheduled_job::list(&state.pool, &auth_user).await?;
    Ok(Json(rows))
}

/// POST /api/scheduled-jobs
pub async fn create(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Json(req): Json<CreateScheduledJobRequest>,
) -> AppResult<Json<ScheduledJob>> {
    let row = services::scheduled_job::create(&state.pool, &auth_user, req).await?;
    Ok(Json(row))
}

/// PUT /api/scheduled-jobs/:id
pub async fn update(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateScheduledJobRequest>,
) -> AppResult<Json<ScheduledJob>> {
    let row = services::scheduled_job::update(&state.pool, &auth_user, id, req).await?;
    Ok(Json(row))
}

/// DELETE /api/scheduled-jobs/:id
pub async fn delete(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    services::scheduled_job::delete(&state.pool, &auth_user, id).await?;
    Ok(Json(serde_json::json!({"message": "Scheduled job deleted"})))
}

// ─── Job Runs ──────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct RunsQuery {
    pub limit: Option<i64>,
}

/// GET /api/scheduled-jobs/:id/runs
pub async fn list_runs(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(job_id): Path<Uuid>,
    Query(q): Query<RunsQuery>,
) -> AppResult<Json<Vec<JobRun>>> {
    let limit = q.limit.unwrap_or(50).min(200);
    let rows = services::scheduled_job::list_runs(&state.pool, &auth_user, job_id, limit).await?;
    Ok(Json(rows))
}

/// GET /api/job-runs/:id
pub async fn get_run(
    _auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(run_id): Path<Uuid>,
) -> AppResult<Json<JobRun>> {
    let run = services::scheduled_job::get_run(&state.pool, run_id).await?;
    Ok(Json(run))
}

/// POST /api/scheduled-jobs/:id/run
/// Manually trigger a job execution
pub async fn trigger_run(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(job_id): Path<Uuid>,
) -> AppResult<Json<JobRun>> {
    let (run, job) = services::scheduled_job::trigger_run(&state.pool, &auth_user, job_id).await?;

    // Dispatch execution in background
    let pool = state.pool.clone();
    let run_id = run.id;
    tokio::spawn(async move {
        crate::services::scheduler::execute_job(&pool, &job, run_id).await;
    });

    Ok(Json(run))
}
