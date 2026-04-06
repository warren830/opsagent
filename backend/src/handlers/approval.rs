use axum::{
    Json,
    extract::{Path, Query, State},
};
use uuid::Uuid;

use crate::AppState;
use crate::error::{AppError, AppResult};
use crate::middleware::auth::AuthUser;
use crate::models::approval::{Approval, ApprovalListQuery};

/// GET /api/approvals
pub async fn list(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Query(query): Query<ApprovalListQuery>,
) -> AppResult<Json<Vec<Approval>>> {
    let approvals = if auth_user.is_super_admin() {
        match &query.status {
            Some(status) => {
                sqlx::query_as::<_, Approval>("SELECT * FROM approvals WHERE status = $1 ORDER BY created_at DESC")
                    .bind(status)
                    .fetch_all(&state.pool)
                    .await?
            }
            None => {
                sqlx::query_as::<_, Approval>("SELECT * FROM approvals ORDER BY created_at DESC")
                    .fetch_all(&state.pool)
                    .await?
            }
        }
    } else {
        let tid = auth_user.tenant_id;
        match &query.status {
            Some(status) => {
                sqlx::query_as::<_, Approval>(
                    r#"SELECT * FROM approvals
                       WHERE tenant_id = $1 AND status = $2
                       ORDER BY created_at DESC"#,
                )
                .bind(tid)
                .bind(status)
                .fetch_all(&state.pool)
                .await?
            }
            None => {
                sqlx::query_as::<_, Approval>("SELECT * FROM approvals WHERE tenant_id = $1 ORDER BY created_at DESC")
                    .bind(tid)
                    .fetch_all(&state.pool)
                    .await?
            }
        }
    };

    Ok(Json(approvals))
}

/// POST /api/approvals/:id/approve
pub async fn approve(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Approval>> {
    // Verify access
    if !auth_user.is_super_admin() {
        let existing = sqlx::query_as::<_, Approval>("SELECT * FROM approvals WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Approval not found".to_string()))?;

        if existing.tenant_id != auth_user.tenant_id {
            return Err(AppError::Forbidden("Access denied".to_string()));
        }

        if existing.status != "pending" {
            return Err(AppError::BadRequest(format!("Approval is already {}", existing.status)));
        }
    }

    let approval = sqlx::query_as::<_, Approval>(
        r#"UPDATE approvals SET
           status = 'approved',
           reviewed_by = $2,
           reviewed_at = NOW()
           WHERE id = $1 AND status = 'pending'
           RETURNING *"#,
    )
    .bind(id)
    .bind(auth_user.user_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Approval not found or already processed".to_string()))?;

    Ok(Json(approval))
}

/// POST /api/approvals/:id/reject
pub async fn reject(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Approval>> {
    // Verify access
    if !auth_user.is_super_admin() {
        let existing = sqlx::query_as::<_, Approval>("SELECT * FROM approvals WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Approval not found".to_string()))?;

        if existing.tenant_id != auth_user.tenant_id {
            return Err(AppError::Forbidden("Access denied".to_string()));
        }

        if existing.status != "pending" {
            return Err(AppError::BadRequest(format!("Approval is already {}", existing.status)));
        }
    }

    let approval = sqlx::query_as::<_, Approval>(
        r#"UPDATE approvals SET
           status = 'rejected',
           reviewed_by = $2,
           reviewed_at = NOW()
           WHERE id = $1 AND status = 'pending'
           RETURNING *"#,
    )
    .bind(id)
    .bind(auth_user.user_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Approval not found or already processed".to_string()))?;

    Ok(Json(approval))
}
