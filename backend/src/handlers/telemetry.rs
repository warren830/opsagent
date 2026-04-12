use axum::{
    Json,
    extract::{Path, State},
};
use uuid::Uuid;

use crate::AppState;
use crate::error::AppResult;
use crate::middleware::auth::AuthUser;
use crate::models::telemetry::{CreateTelemetryRequest, TelemetryConfig, UpdateTelemetryRequest};
use crate::services;

/// GET /api/telemetry — list all configs for the user's tenant
pub async fn list(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
) -> AppResult<Json<Vec<TelemetryConfig>>> {
    let rows = services::telemetry::list(&state.pool, &auth_user).await?;
    Ok(Json(rows))
}

/// POST /api/telemetry — create a new config
pub async fn create(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Json(req): Json<CreateTelemetryRequest>,
) -> AppResult<Json<TelemetryConfig>> {
    let row = services::telemetry::create(&state.pool, &auth_user, req).await?;
    Ok(Json(row))
}

/// PUT /api/telemetry/:id — update a config
pub async fn update(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateTelemetryRequest>,
) -> AppResult<Json<TelemetryConfig>> {
    let row = services::telemetry::update(&state.pool, &auth_user, id, req).await?;
    Ok(Json(row))
}

/// DELETE /api/telemetry/:id
pub async fn delete(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    services::telemetry::delete(&state.pool, &auth_user, id).await?;
    Ok(Json(serde_json::json!({"message": "Telemetry config deleted"})))
}

/// POST /api/telemetry/test (mock)
pub async fn test_connection(_auth_user: axum::Extension<AuthUser>) -> AppResult<Json<serde_json::Value>> {
    Ok(Json(serde_json::json!({
        "status": "ok",
        "message": "Telemetry connection test successful"
    })))
}
