use axum::{
    extract::{Path, State},
    Json,
};
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::middleware::auth::AuthUser;
use crate::models::scheduled_job::{
    CreateScheduledJobRequest, ScheduledJob, UpdateScheduledJobRequest,
};
use crate::AppState;

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
        return Err(AppError::BadRequest(
            "Cron expression is required".to_string(),
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
        r#"INSERT INTO scheduled_jobs (name, cron_expression, timezone, query, enabled, auto_jira, targets, tenant_id, user_id, created_by, visibility)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
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
    let existing =
        sqlx::query_as::<_, ScheduledJob>("SELECT * FROM scheduled_jobs WHERE id = $1")
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
    let existing =
        sqlx::query_as::<_, ScheduledJob>("SELECT * FROM scheduled_jobs WHERE id = $1")
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

    Ok(Json(
        serde_json::json!({"message": "Scheduled job deleted"}),
    ))
}
