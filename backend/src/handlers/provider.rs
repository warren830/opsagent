use axum::{
    Json,
    extract::{Path, State},
};
use uuid::Uuid;

use crate::AppState;
use crate::error::AppResult;
use crate::middleware::auth::AuthUser;
use crate::models::provider::{CreateProviderRequest, Provider, ProviderTypeOption, UpdateProviderRequest};
use crate::services;

/// GET /api/providers — list all model configurations for the current tenant
pub async fn list(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
) -> AppResult<Json<Vec<Provider>>> {
    let rows = services::provider::list(&state.pool, &auth_user).await?;
    Ok(Json(rows))
}

/// GET /api/providers/types — available provider types based on environment
pub async fn available_types(State(state): State<AppState>) -> AppResult<Json<Vec<ProviderTypeOption>>> {
    let types = services::provider::available_types(state.config.env.is_local());
    Ok(Json(types))
}

/// POST /api/providers — create a new model configuration
pub async fn create(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Json(req): Json<CreateProviderRequest>,
) -> AppResult<Json<Provider>> {
    let row = services::provider::create(&state.pool, &auth_user, req).await?;
    Ok(Json(row))
}

/// PUT /api/providers/:id — update a model configuration
pub async fn update(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateProviderRequest>,
) -> AppResult<Json<Provider>> {
    let row = services::provider::update(&state.pool, &auth_user, id, req).await?;
    Ok(Json(row))
}

/// DELETE /api/providers/:id — delete a model configuration
pub async fn delete(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    services::provider::delete(&state.pool, &auth_user, id).await?;
    Ok(Json(serde_json::json!({"message": "Provider deleted"})))
}
