use axum::{
    Json,
    extract::{Path, State},
};
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::error::{AppError, AppResult};
use crate::middleware::auth::AuthUser;
use crate::models::channel::Channel;
use crate::services::jira::JiraClient;

// ─── Request / Response types ───────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateIssueRequest {
    pub summary: String,
    #[serde(default = "default_description")]
    pub description: String,
    pub issue_type: Option<String>,
    pub labels: Option<Vec<String>>,
}

fn default_description() -> String {
    "Created via OpenOps".to_string()
}

#[derive(Debug, Serialize)]
pub struct CreateIssueResponse {
    pub key: String,
    pub id: String,
    pub url: String,
}

#[derive(Debug, Deserialize)]
pub struct TransitionRequest {
    pub status: String,
    pub comment: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CommentRequest {
    pub comment: String,
}

// ─── Helper: get Jira client for tenant ─────────────────────────────────────

async fn get_jira_client(state: &AppState, auth_user: &AuthUser) -> AppResult<JiraClient> {
    let channel = sqlx::query_as::<_, Channel>(
        "SELECT * FROM channels WHERE platform = 'jira' AND enabled = true AND tenant_id = $1 LIMIT 1",
    )
    .bind(auth_user.tenant_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("No Jira integration configured. Add one in Settings → Channels.".into()))?;

    JiraClient::from_credentials(&channel.credentials)
}

// ─── Handlers ───────────────────────────────────────────────────────────────

/// POST /api/jira/create — Create a Jira issue
pub async fn create_issue(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Json(req): Json<CreateIssueRequest>,
) -> AppResult<Json<CreateIssueResponse>> {
    let client = get_jira_client(&state, &auth_user).await?;

    let issue = client
        .create_issue(&req.summary, &req.description, req.issue_type.as_deref(), req.labels)
        .await?;

    let url = format!("{}/browse/{}", client.base_url, issue.key);

    tracing::info!(
        "Jira issue {} created by user {} (tenant {:?})",
        issue.key,
        auth_user.user_id,
        auth_user.tenant_id
    );

    Ok(Json(CreateIssueResponse {
        key: issue.key,
        id: issue.id,
        url,
    }))
}

/// POST /api/jira/:key/transition — Transition issue status
pub async fn transition_issue(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(key): Path<String>,
    Json(req): Json<TransitionRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let client = get_jira_client(&state, &auth_user).await?;

    client
        .transition_issue(&key, &req.status, req.comment.as_deref())
        .await?;

    tracing::info!(
        "Jira issue {} transitioned to '{}' by user {}",
        key,
        req.status,
        auth_user.user_id
    );

    Ok(Json(serde_json::json!({
        "success": true,
        "key": key,
        "status": req.status
    })))
}

/// POST /api/jira/:key/comment — Add comment to issue
pub async fn add_comment(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(key): Path<String>,
    Json(req): Json<CommentRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let client = get_jira_client(&state, &auth_user).await?;

    client.add_comment(&key, &req.comment).await?;

    Ok(Json(serde_json::json!({
        "success": true,
        "key": key
    })))
}

/// GET /api/jira/:key — Get issue details
pub async fn get_issue(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    let client = get_jira_client(&state, &auth_user).await?;

    let issue = client.get_issue(&key).await?;

    Ok(Json(issue))
}
