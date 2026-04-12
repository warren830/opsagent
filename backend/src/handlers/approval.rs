use axum::{
    Json,
    extract::{Path, Query, State},
};
use uuid::Uuid;

use crate::AppState;
use crate::error::AppResult;
use crate::middleware::auth::AuthUser;
use crate::models::approval::{Approval, ApprovalListQuery};
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
