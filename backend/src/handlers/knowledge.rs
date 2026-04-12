use axum::{
    Json,
    extract::{Path, State},
};
use uuid::Uuid;

use crate::AppState;
use crate::error::AppResult;
use crate::middleware::auth::AuthUser;
use crate::models::knowledge::{CreateKnowledgeRequest, KnowledgeFile, UpdateKnowledgeRequest};
use crate::services;

/// GET /api/knowledge
pub async fn list(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
) -> AppResult<Json<Vec<KnowledgeFile>>> {
    let rows = services::knowledge::list(&state.pool, &auth_user).await?;
    Ok(Json(rows))
}

/// POST /api/knowledge
pub async fn create(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Json(req): Json<CreateKnowledgeRequest>,
) -> AppResult<Json<KnowledgeFile>> {
    let row = services::knowledge::create(&state.pool, &auth_user, req).await?;
    Ok(Json(row))
}

/// PUT /api/knowledge/:id
pub async fn update(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateKnowledgeRequest>,
) -> AppResult<Json<KnowledgeFile>> {
    let row = services::knowledge::update(&state.pool, &auth_user, id, req).await?;
    Ok(Json(row))
}

/// DELETE /api/knowledge/:id
pub async fn delete(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    services::knowledge::delete(&state.pool, &auth_user, id).await?;
    Ok(Json(serde_json::json!({"message": "Knowledge file deleted"})))
}
