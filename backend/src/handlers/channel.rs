use axum::{
    Json,
    extract::{Path, State},
};
use uuid::Uuid;

use crate::AppState;
use crate::error::AppResult;
use crate::middleware::auth::AuthUser;
use crate::models::channel::{ChannelWithTenants, CreateChannelRequest, UpdateChannelRequest};
use crate::services;

/// GET /api/channels
pub async fn list(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
) -> AppResult<Json<Vec<ChannelWithTenants>>> {
    let rows = services::channel::list(&state.pool, &auth_user).await?;
    Ok(Json(rows))
}

/// POST /api/channels
pub async fn create(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Json(req): Json<CreateChannelRequest>,
) -> AppResult<Json<ChannelWithTenants>> {
    let row = services::channel::create(&state.pool, &auth_user, req).await?;
    Ok(Json(row))
}

/// PUT /api/channels/:id
pub async fn update(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateChannelRequest>,
) -> AppResult<Json<ChannelWithTenants>> {
    let row = services::channel::update(&state.pool, &auth_user, id, req).await?;
    Ok(Json(row))
}

/// DELETE /api/channels/:id
pub async fn delete(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    services::channel::delete(&state.pool, &auth_user, id).await?;
    Ok(Json(serde_json::json!({"message": "Channel deleted"})))
}
