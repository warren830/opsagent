use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;
use uuid::Uuid;

use crate::AppState;
use crate::error::{AppError, AppResult};
use crate::middleware::auth::AuthUser;
use crate::models::scheduled_job::{CreateScheduledJobRequest, JobRun, ScheduledJob, UpdateScheduledJobRequest};

/// GET /api/scheduled-jobs
/// Super admin: all. Normal user: own private + tenant public
pub async fn list(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
) -> AppResult<Json<Vec<ScheduledJob>>> {
    let rows = if auth_user.is_super_admin() {
        sqlx::query_as::<_, ScheduledJob>("SELECT * FROM scheduled_jobs ORDER BY name")
            .fetch_all(&state.pool)
            .await?
    } else {
        sqlx::query_as::<_, ScheduledJob>(
            r#"SELECT * FROM scheduled_jobs
               WHERE (user_id = $1) OR (user_id IS NULL AND tenant_id IS NOT DISTINCT FROM $2)
               ORDER BY name"#,
        )
        .bind(auth_user.user_id)
        .bind(auth_user.tenant_id)
        .fetch_all(&state.pool)
        .await?
    };
    Ok(Json(rows))
}

/// POST /api/scheduled-jobs
pub async fn create(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Json(req): Json<CreateScheduledJobRequest>,
) -> AppResult<Json<ScheduledJob>> {
    if req.name.trim().is_empty() {
        return Err(AppError::BadRequest("Name is required".to_string()));
    }
    if req.cron_expression.trim().is_empty() {
        return Err(AppError::BadRequest("Cron expression is required".to_string()));
    }

    let job_type = match req.job_type.as_str() {
        "builtin" | "agent" | "skill" => req.job_type.clone(),
        _ => "agent".to_string(),
    };

    if job_type == "skill" && req.skill_path.is_none() {
        return Err(AppError::BadRequest(
            "Skill path is required for skill jobs".to_string(),
        ));
    }

    let visibility = match req.visibility.as_str() {
        "public" | "private" => req.visibility.clone(),
        _ => "public".to_string(),
    };

    let tenant_id = auth_user.tenant_id;
    let user_id = if visibility == "private" {
        Some(auth_user.user_id)
    } else {
        None
    };

    let row = sqlx::query_as::<_, ScheduledJob>(
        r#"INSERT INTO scheduled_jobs (name, cron_expression, timezone, query, enabled, auto_jira, targets,
           tenant_id, user_id, created_by, visibility, job_type, skill_path, skill_params)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
           RETURNING *"#,
    )
    .bind(&req.name)
    .bind(&req.cron_expression)
    .bind(&req.timezone)
    .bind(&req.query)
    .bind(req.enabled)
    .bind(req.auto_jira)
    .bind(&req.targets)
    .bind(tenant_id)
    .bind(user_id)
    .bind(auth_user.user_id)
    .bind(&visibility)
    .bind(&job_type)
    .bind(&req.skill_path)
    .bind(&req.skill_params)
    .fetch_one(&state.pool)
    .await?;

    Ok(Json(row))
}

/// PUT /api/scheduled-jobs/:id
pub async fn update(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateScheduledJobRequest>,
) -> AppResult<Json<ScheduledJob>> {
    let existing = sqlx::query_as::<_, ScheduledJob>("SELECT * FROM scheduled_jobs WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Scheduled job not found".to_string()))?;

    if !auth_user.is_super_admin() {
        let has_access = existing.user_id == Some(auth_user.user_id)
            || (existing.visibility == "public" && existing.tenant_id == auth_user.tenant_id);
        if !has_access {
            return Err(AppError::Forbidden("Access denied".to_string()));
        }
    }

    let row = sqlx::query_as::<_, ScheduledJob>(
        r#"UPDATE scheduled_jobs SET
           name = COALESCE($2, name),
           cron_expression = COALESCE($3, cron_expression),
           timezone = COALESCE($4, timezone),
           query = COALESCE($5, query),
           enabled = COALESCE($6, enabled),
           auto_jira = COALESCE($7, auto_jira),
           targets = COALESCE($8, targets),
           job_type = COALESCE($9, job_type),
           skill_path = COALESCE($10, skill_path),
           skill_params = COALESCE($11, skill_params),
           updated_at = NOW()
           WHERE id = $1
           RETURNING *"#,
    )
    .bind(id)
    .bind(&req.name)
    .bind(&req.cron_expression)
    .bind(&req.timezone)
    .bind(&req.query)
    .bind(req.enabled)
    .bind(req.auto_jira)
    .bind(&req.targets)
    .bind(&req.job_type)
    .bind(&req.skill_path)
    .bind(&req.skill_params)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Scheduled job not found".to_string()))?;

    Ok(Json(row))
}

/// DELETE /api/scheduled-jobs/:id
pub async fn delete(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let existing = sqlx::query_as::<_, ScheduledJob>("SELECT * FROM scheduled_jobs WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Scheduled job not found".to_string()))?;

    if !auth_user.is_super_admin() {
        let has_access = existing.user_id == Some(auth_user.user_id)
            || (existing.visibility == "public" && existing.tenant_id == auth_user.tenant_id);
        if !has_access {
            return Err(AppError::Forbidden("Access denied".to_string()));
        }
    }

    sqlx::query("DELETE FROM scheduled_jobs WHERE id = $1")
        .bind(id)
        .execute(&state.pool)
        .await?;

    Ok(Json(serde_json::json!({"message": "Scheduled job deleted"})))
}

// ─── Job Runs ──────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct RunsQuery {
    pub limit: Option<i64>,
}

/// GET /api/scheduled-jobs/:id/runs
/// List execution history for a job
pub async fn list_runs(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(job_id): Path<Uuid>,
    Query(q): Query<RunsQuery>,
) -> AppResult<Json<Vec<JobRun>>> {
    // Verify job access
    let job = sqlx::query_as::<_, ScheduledJob>("SELECT * FROM scheduled_jobs WHERE id = $1")
        .bind(job_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Scheduled job not found".to_string()))?;

    if !auth_user.is_super_admin() {
        let has_access = job.user_id == Some(auth_user.user_id)
            || (job.visibility == "public" && job.tenant_id == auth_user.tenant_id);
        if !has_access {
            return Err(AppError::Forbidden("Access denied".to_string()));
        }
    }

    let limit = q.limit.unwrap_or(50).min(200);
    let rows = sqlx::query_as::<_, JobRun>(
        "SELECT * FROM job_runs WHERE job_id = $1 ORDER BY started_at DESC NULLS LAST LIMIT $2",
    )
    .bind(job_id)
    .bind(limit)
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(rows))
}

/// GET /api/job-runs/:id
/// Get a single run with full output
pub async fn get_run(
    _auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(run_id): Path<Uuid>,
) -> AppResult<Json<JobRun>> {
    let run = sqlx::query_as::<_, JobRun>("SELECT * FROM job_runs WHERE id = $1")
        .bind(run_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Job run not found".to_string()))?;

    Ok(Json(run))
}

/// POST /api/scheduled-jobs/:id/run
/// Manually trigger a job execution
pub async fn trigger_run(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(job_id): Path<Uuid>,
) -> AppResult<Json<JobRun>> {
    let job = sqlx::query_as::<_, ScheduledJob>("SELECT * FROM scheduled_jobs WHERE id = $1")
        .bind(job_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Scheduled job not found".to_string()))?;

    if !auth_user.is_super_admin() {
        let has_access = job.user_id == Some(auth_user.user_id)
            || (job.visibility == "public" && job.tenant_id == auth_user.tenant_id);
        if !has_access {
            return Err(AppError::Forbidden("Access denied".to_string()));
        }
    }

    // Create a pending run record
    let run = sqlx::query_as::<_, JobRun>(
        r#"INSERT INTO job_runs (job_id, status, trigger, tenant_id)
           VALUES ($1, 'pending', 'manual', $2)
           RETURNING *"#,
    )
    .bind(job_id)
    .bind(job.tenant_id)
    .fetch_one(&state.pool)
    .await?;

    // Dispatch execution in background
    let pool = state.pool.clone();
    let run_id = run.id;
    tokio::spawn(async move {
        crate::services::scheduler::execute_job(&pool, &job, run_id).await;
    });

    Ok(Json(run))
}
