use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::middleware::auth::AuthUser;
use crate::models::approval::Approval;

/// List approvals visible to the authenticated user.
/// Super admins see all approvals; other users see only their tenant's approvals.
/// Optionally filter by status.
pub async fn list(
    pool: &PgPool,
    auth_user: &AuthUser,
    status: Option<&str>,
) -> AppResult<Vec<Approval>> {
    let approvals = if auth_user.is_super_admin() {
        match status {
            Some(status) => {
                sqlx::query_as::<_, Approval>(
                    "SELECT * FROM approvals WHERE status = $1 ORDER BY created_at DESC",
                )
                .bind(status)
                .fetch_all(pool)
                .await?
            }
            None => {
                sqlx::query_as::<_, Approval>(
                    "SELECT * FROM approvals ORDER BY created_at DESC",
                )
                .fetch_all(pool)
                .await?
            }
        }
    } else {
        let tid = auth_user.tenant_id;
        match status {
            Some(status) => {
                sqlx::query_as::<_, Approval>(
                    r#"SELECT * FROM approvals
                       WHERE tenant_id = $1 AND status = $2
                       ORDER BY created_at DESC"#,
                )
                .bind(tid)
                .bind(status)
                .fetch_all(pool)
                .await?
            }
            None => {
                sqlx::query_as::<_, Approval>(
                    "SELECT * FROM approvals WHERE tenant_id = $1 ORDER BY created_at DESC",
                )
                .bind(tid)
                .fetch_all(pool)
                .await?
            }
        }
    };

    Ok(approvals)
}

/// Approve a pending approval request.
/// Non-super_admin users can only approve requests belonging to their tenant.
pub async fn approve(
    pool: &PgPool,
    auth_user: &AuthUser,
    id: Uuid,
) -> AppResult<Approval> {
    // Verify access
    if !auth_user.is_super_admin() {
        let existing = sqlx::query_as::<_, Approval>("SELECT * FROM approvals WHERE id = $1")
            .bind(id)
            .fetch_optional(pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Approval not found".to_string()))?;

        if existing.tenant_id != auth_user.tenant_id {
            return Err(AppError::Forbidden("Access denied".to_string()));
        }

        if existing.status != "pending" {
            return Err(AppError::BadRequest(format!(
                "Approval is already {}",
                existing.status
            )));
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
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Approval not found or already processed".to_string()))?;

    Ok(approval)
}

/// Reject a pending approval request.
/// Non-super_admin users can only reject requests belonging to their tenant.
pub async fn reject(
    pool: &PgPool,
    auth_user: &AuthUser,
    id: Uuid,
) -> AppResult<Approval> {
    // Verify access
    if !auth_user.is_super_admin() {
        let existing = sqlx::query_as::<_, Approval>("SELECT * FROM approvals WHERE id = $1")
            .bind(id)
            .fetch_optional(pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Approval not found".to_string()))?;

        if existing.tenant_id != auth_user.tenant_id {
            return Err(AppError::Forbidden("Access denied".to_string()));
        }

        if existing.status != "pending" {
            return Err(AppError::BadRequest(format!(
                "Approval is already {}",
                existing.status
            )));
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
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Approval not found or already processed".to_string()))?;

    Ok(approval)
}
