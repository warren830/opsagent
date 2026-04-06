use axum::{
    Json,
    extract::{Path, State},
};
use uuid::Uuid;

use crate::AppState;
use crate::error::{AppError, AppResult};
use crate::middleware::auth::AuthUser;
use crate::models::pipeline::{CreatePipelineRepoRequest, PipelineRepo, UpdatePipelineRepoRequest};

/// GET /api/pipeline/repos
pub async fn list(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
) -> AppResult<Json<Vec<PipelineRepo>>> {
    let rows = if auth_user.is_super_admin() {
        sqlx::query_as::<_, PipelineRepo>("SELECT * FROM pipeline_repos ORDER BY name")
            .fetch_all(&state.pool)
            .await?
    } else {
        sqlx::query_as::<_, PipelineRepo>("SELECT * FROM pipeline_repos WHERE tenant_id = $1 ORDER BY name")
            .bind(auth_user.tenant_id)
            .fetch_all(&state.pool)
            .await?
    };
    Ok(Json(rows))
}

/// POST /api/pipeline/repos
pub async fn create(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Json(req): Json<CreatePipelineRepoRequest>,
) -> AppResult<Json<PipelineRepo>> {
    if req.name.trim().is_empty() {
        return Err(AppError::BadRequest("Name is required".to_string()));
    }
    if req.repository.trim().is_empty() {
        return Err(AppError::BadRequest("Repository is required".to_string()));
    }

    let row = sqlx::query_as::<_, PipelineRepo>(
        r#"INSERT INTO pipeline_repos (repo_id, name, repository, token_secret_arn, description, enabled, tenant_id)
           VALUES ($1, $2, $3, $4, $5, $6, $7)
           RETURNING *"#,
    )
    .bind(&req.repo_id)
    .bind(&req.name)
    .bind(&req.repository)
    .bind(&req.token_secret_arn)
    .bind(&req.description)
    .bind(req.enabled)
    .bind(auth_user.tenant_id)
    .fetch_one(&state.pool)
    .await?;

    Ok(Json(row))
}

/// PUT /api/pipeline/repos/:id
pub async fn update(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdatePipelineRepoRequest>,
) -> AppResult<Json<PipelineRepo>> {
    if !auth_user.is_super_admin() {
        let existing = sqlx::query_as::<_, PipelineRepo>("SELECT * FROM pipeline_repos WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Pipeline repo not found".to_string()))?;
        if existing.tenant_id != auth_user.tenant_id {
            return Err(AppError::Forbidden("Access denied".to_string()));
        }
    }

    let row = sqlx::query_as::<_, PipelineRepo>(
        r#"UPDATE pipeline_repos SET
           name = COALESCE($2, name),
           repository = COALESCE($3, repository),
           token_secret_arn = COALESCE($4, token_secret_arn),
           description = COALESCE($5, description),
           enabled = COALESCE($6, enabled),
           updated_at = NOW()
           WHERE id = $1
           RETURNING *"#,
    )
    .bind(id)
    .bind(&req.name)
    .bind(&req.repository)
    .bind(&req.token_secret_arn)
    .bind(&req.description)
    .bind(req.enabled)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Pipeline repo not found".to_string()))?;

    Ok(Json(row))
}

/// DELETE /api/pipeline/repos/:id
pub async fn delete(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    if !auth_user.is_super_admin() {
        let existing = sqlx::query_as::<_, PipelineRepo>("SELECT * FROM pipeline_repos WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Pipeline repo not found".to_string()))?;
        if existing.tenant_id != auth_user.tenant_id {
            return Err(AppError::Forbidden("Access denied".to_string()));
        }
    }

    let result = sqlx::query("DELETE FROM pipeline_repos WHERE id = $1")
        .bind(id)
        .execute(&state.pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Pipeline repo not found".to_string()));
    }

    Ok(Json(serde_json::json!({"message": "Pipeline repo deleted"})))
}
