use axum::{
    extract::{Path, State},
    Json,
};
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::middleware::auth::AuthUser;
use crate::models::channel::{Channel, CreateChannelRequest, UpdateChannelRequest};
use crate::AppState;

/// GET /api/channels
pub async fn list(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
) -> AppResult<Json<Vec<Channel>>> {
    let rows = if auth_user.is_super_admin() {
        sqlx::query_as::<_, Channel>("SELECT * FROM channels ORDER BY name")
            .fetch_all(&state.pool)
            .await?
    } else {
        sqlx::query_as::<_, Channel>(
            "SELECT * FROM channels WHERE tenant_id = $1 ORDER BY name",
        )
        .bind(auth_user.tenant_id)
        .fetch_all(&state.pool)
        .await?
    };
    Ok(Json(rows))
}

/// POST /api/channels
pub async fn create(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Json(req): Json<CreateChannelRequest>,
) -> AppResult<Json<Channel>> {
    if req.name.trim().is_empty() {
        return Err(AppError::BadRequest("Name is required".to_string()));
    }
    if req.platform.trim().is_empty() {
        return Err(AppError::BadRequest("Platform is required".to_string()));
    }

    let row = sqlx::query_as::<_, Channel>(
        r#"INSERT INTO channels (platform, name, credentials, settings, enabled, tenant_id)
           VALUES ($1, $2, $3, $4, $5, $6)
           RETURNING *"#,
    )
    .bind(&req.platform)
    .bind(&req.name)
    .bind(&req.credentials)
    .bind(&req.settings)
    .bind(req.enabled)
    .bind(auth_user.tenant_id)
    .fetch_one(&state.pool)
    .await?;

    Ok(Json(row))
}

/// PUT /api/channels/:id
pub async fn update(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateChannelRequest>,
) -> AppResult<Json<Channel>> {
    if !auth_user.is_super_admin() {
        let existing = sqlx::query_as::<_, Channel>("SELECT * FROM channels WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Channel not found".to_string()))?;
        if existing.tenant_id != auth_user.tenant_id {
            return Err(AppError::Forbidden("Access denied".to_string()));
        }
    }

    let row = sqlx::query_as::<_, Channel>(
        r#"UPDATE channels SET
           platform = COALESCE($2, platform),
           name = COALESCE($3, name),
           credentials = COALESCE($4, credentials),
           settings = COALESCE($5, settings),
           enabled = COALESCE($6, enabled),
           updated_at = NOW()
           WHERE id = $1
           RETURNING *"#,
    )
    .bind(id)
    .bind(&req.platform)
    .bind(&req.name)
    .bind(&req.credentials)
    .bind(&req.settings)
    .bind(req.enabled)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Channel not found".to_string()))?;

    Ok(Json(row))
}

/// DELETE /api/channels/:id
pub async fn delete(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    if !auth_user.is_super_admin() {
        let existing = sqlx::query_as::<_, Channel>("SELECT * FROM channels WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Channel not found".to_string()))?;
        if existing.tenant_id != auth_user.tenant_id {
            return Err(AppError::Forbidden("Access denied".to_string()));
        }
    }

    let result = sqlx::query("DELETE FROM channels WHERE id = $1")
        .bind(id)
        .execute(&state.pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Channel not found".to_string()));
    }

    Ok(Json(serde_json::json!({"message": "Channel deleted"})))
}
