use axum::{
    Json,
    extract::{Path, State},
};
use serde::Deserialize;
use uuid::Uuid;

use crate::AppState;
use crate::error::AppResult;
use crate::middleware::auth::AuthUser;
use crate::models::pipeline::{CreatePipelineRepoRequest, PipelineRepo, UpdatePipelineRepoRequest};
use crate::services;
use crate::services::pipeline::TestConnectionResult;

/// GET /api/pipeline/repos
pub async fn list(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
) -> AppResult<Json<Vec<PipelineRepo>>> {
    let rows = services::pipeline::list(&state.pool, &auth_user).await?;
    Ok(Json(rows))
}

/// POST /api/pipeline/repos
pub async fn create(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Json(req): Json<CreatePipelineRepoRequest>,
) -> AppResult<Json<PipelineRepo>> {
    let row = services::pipeline::create(&state.pool, &auth_user, req).await?;
    Ok(Json(row))
}

/// PUT /api/pipeline/repos/:id
pub async fn update(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdatePipelineRepoRequest>,
) -> AppResult<Json<PipelineRepo>> {
    let row = services::pipeline::update(&state.pool, &auth_user, id, req).await?;
    Ok(Json(row))
}

/// DELETE /api/pipeline/repos/:id
pub async fn delete(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    services::pipeline::delete(&state.pool, &auth_user, id).await?;
    Ok(Json(serde_json::json!({"message": "Pipeline repo deleted"})))
}

/// POST /api/pipeline/repos/:id/test
pub async fn test_connection(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<TestConnectionResult>> {
    let result = services::pipeline::test_connection(&state.pool, &auth_user, id).await?;
    Ok(Json(result))
}

#[derive(Debug, Deserialize)]
pub struct TestInlineRequest {
    pub repository: String,
    pub token: Option<String>,
}

/// POST /api/pipeline/repos/test
pub async fn test_connection_inline(
    _auth_user: axum::Extension<AuthUser>,
    State(_state): State<AppState>,
    Json(req): Json<TestInlineRequest>,
) -> AppResult<Json<TestConnectionResult>> {
    let result =
        services::pipeline::test_connection_inline(&req.repository, req.token.as_deref()).await?;
    Ok(Json(result))
}
