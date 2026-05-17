use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde_json::json;
use uuid::Uuid;

use crate::AppState;
use crate::error::AppResult;
use crate::middleware::auth::AuthUser;
use crate::models::approval::{
    Approval, ApprovalListQuery, CreateApprovalRequest, MarkResultRequest, UpdateApprovalRequest,
};
use crate::services;

/// GET /api/approvals
pub async fn list(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Query(query): Query<ApprovalListQuery>,
) -> AppResult<Json<Vec<Approval>>> {
    let approvals = services::approval::list(&state.pool, &auth_user, query.status.as_deref()).await?;
    Ok(Json(approvals))
}

/// GET /api/approvals/count — returns {"pending": n}
pub async fn count(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
) -> AppResult<Json<serde_json::Value>> {
    let n = services::approval::count(&state.pool, &auth_user).await?;
    Ok(Json(json!({ "pending": n })))
}

/// GET /api/approvals/jira-url — returns {"url": "https://..."} or {"url": null}
pub async fn jira_url(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
) -> AppResult<Json<serde_json::Value>> {
    let url = services::approval::get_jira_url(&state.pool, &auth_user).await?;
    Ok(Json(json!({ "url": url })))
}

/// POST /api/approvals
pub async fn create(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Json(req): Json<CreateApprovalRequest>,
) -> AppResult<Json<Approval>> {
    let approval = services::approval::create(&state.pool, &auth_user, req).await?;
    Ok(Json(approval))
}

/// PUT /api/approvals/:id
pub async fn update(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateApprovalRequest>,
) -> AppResult<Json<Approval>> {
    let approval = services::approval::update(&state.pool, &auth_user, id, req.jira_key).await?;
    Ok(Json(approval))
}

/// POST /api/approvals/:id/mark
pub async fn mark_result(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<MarkResultRequest>,
) -> AppResult<Json<Approval>> {
    let approval =
        services::approval::mark_result(&state.pool, &auth_user, id, req.success).await?;
    Ok(Json(approval))
}

/// POST /api/approvals/:id/withdraw
pub async fn withdraw(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Approval>> {
    let approval = services::approval::withdraw(&state.pool, &auth_user, id).await?;
    Ok(Json(approval))
}

/// POST /api/approvals/:id/approve
pub async fn approve(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Approval>> {
    let approval = services::approval::approve(&state.pool, &auth_user, id).await?;
    Ok(Json(approval))
}

/// POST /api/approvals/:id/reject
pub async fn reject(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Approval>> {
    let approval = services::approval::reject(&state.pool, &auth_user, id).await?;
    Ok(Json(approval))
}
