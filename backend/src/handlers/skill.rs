use axum::{
    Json,
    extract::{Path, State},
};
use serde::Deserialize;
use uuid::Uuid;

use crate::AppState;
use crate::error::AppResult;
use crate::middleware::auth::AuthUser;
use crate::models::skill::Skill;
use crate::services;

// ─── Request DTOs ───────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct DiscoverRequest {
    pub git_url: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateSkillRequest {
    pub git_url: String,
    pub selected: Vec<String>,
    #[serde(default = "default_visibility")]
    pub visibility: String,
}

fn default_visibility() -> String {
    "private".to_string()
}

// ─── Handlers ───────────────────────────────────────────────────────────────

/// GET /api/skills
pub async fn list(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
) -> AppResult<Json<Vec<Skill>>> {
    let skills = services::skill::list(&state.pool, &auth_user).await?;
    Ok(Json(skills))
}

/// POST /api/skills/discover
pub async fn discover(
    _auth_user: axum::Extension<AuthUser>,
    Json(req): Json<DiscoverRequest>,
) -> AppResult<Json<services::skill::DiscoverResponse>> {
    let resp = services::skill::discover(&req.git_url).await?;
    Ok(Json(resp))
}

/// POST /api/skills
pub async fn create(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Json(req): Json<CreateSkillRequest>,
) -> AppResult<Json<Vec<Skill>>> {
    let installed = services::skill::create(
        &state.pool,
        &auth_user,
        &req.git_url,
        &req.selected,
        &req.visibility,
        &state.config.claude_work_dir,
    )
    .await?;
    Ok(Json(installed))
}

/// PUT /api/skills/:id
pub async fn update(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Skill>> {
    let skill = services::skill::update(&state.pool, &auth_user, id).await?;
    Ok(Json(skill))
}

/// DELETE /api/skills/:id
pub async fn delete(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    services::skill::delete(&state.pool, &auth_user, id).await?;
    Ok(Json(serde_json::json!({ "message": "Skill removed" })))
}
