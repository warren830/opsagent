use axum::{
    Json,
    extract::{Path, State},
};
use serde::Deserialize;
use uuid::Uuid;

use crate::AppState;
use crate::error::{AppError, AppResult};
use crate::middleware::auth::AuthUser;
use crate::models::channel::Channel;
use crate::models::knowledge::{CreateKnowledgeRequest, KnowledgeFile, UpdateKnowledgeRequest};
use crate::services;
use crate::services::jira::JiraClient;
use crate::services::knowledge_sync::SyncResult;

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

// ─── Knowledge Sync ────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct SyncRequest {
    pub source: String,             // "jira" | "confluence"
    pub filter: String,             // JQL for jira, space key for confluence
    pub max_results: Option<usize>, // default 50
    // Inline Atlassian credentials (no channel dependency)
    pub base_url: String,           // e.g. "https://yoursite.atlassian.net"
    pub email: String,
    pub api_token: String,
}

/// POST /api/knowledge/sync
pub async fn sync(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Json(req): Json<SyncRequest>,
) -> AppResult<Json<SyncResult>> {
    // Validate source
    if req.source != "jira" && req.source != "confluence" {
        return Err(AppError::BadRequest(
            "source must be 'jira' or 'confluence'".into(),
        ));
    }

    // Build JiraClient from inline credentials
    let creds = serde_json::json!({
        "base_url": req.base_url,
        "email": req.email,
        "api_token": req.api_token,
    });
    let client = JiraClient::from_credentials(&creds)?;
    let max_results = req.max_results.unwrap_or(50);

    let result = match req.source.as_str() {
        "jira" => {
            services::knowledge_sync::sync_jira(
                &state.pool,
                &auth_user,
                &client,
                &req.filter,
                max_results,
            )
            .await?
        }
        "confluence" => {
            services::knowledge_sync::sync_confluence(
                &state.pool,
                &auth_user,
                &client,
                &req.filter,
                max_results,
            )
            .await?
        }
        _ => unreachable!(),
    };

    Ok(Json(result))
}
