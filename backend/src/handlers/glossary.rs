use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;
use uuid::Uuid;

use crate::AppState;
use crate::error::AppResult;
use crate::middleware::auth::AuthUser;
use crate::models::glossary::{CreateGlossaryRequest, GlossaryEntry, UpdateGlossaryRequest};
use crate::services;

#[derive(Debug, Deserialize)]
pub struct GlossaryListQuery {
    pub q: Option<String>,
}

/// GET /api/glossary
pub async fn list(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Query(query): Query<GlossaryListQuery>,
) -> AppResult<Json<Vec<GlossaryEntry>>> {
    let entries = services::glossary::list(&state.pool, &auth_user, query.q.as_deref()).await?;
    Ok(Json(entries))
}

/// POST /api/glossary
pub async fn create(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Json(req): Json<CreateGlossaryRequest>,
) -> AppResult<Json<GlossaryEntry>> {
    let entry = services::glossary::create(&state.pool, &auth_user, req).await?;
    Ok(Json(entry))
}

/// PUT /api/glossary/:id
pub async fn update(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateGlossaryRequest>,
) -> AppResult<Json<GlossaryEntry>> {
    let entry = services::glossary::update(&state.pool, &auth_user, id, req).await?;
    Ok(Json(entry))
}

/// DELETE /api/glossary/:id
pub async fn delete(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    services::glossary::delete(&state.pool, &auth_user, id).await?;
    Ok(Json(serde_json::json!({"message": "Glossary entry deleted"})))
}
