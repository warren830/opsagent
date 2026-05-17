use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::middleware::auth::AuthUser;
use crate::models::approval::{Approval, CreateApprovalRequest};

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

/// Count pending approvals for the authenticated user's tenant.
pub async fn count(pool: &PgPool, auth_user: &AuthUser) -> AppResult<i64> {
    let n: i64 = if auth_user.is_super_admin() {
        sqlx::query_scalar("SELECT COUNT(*) FROM approvals WHERE status = 'pending'")
            .fetch_one(pool)
            .await?
    } else {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM approvals WHERE status = 'pending' AND tenant_id = $1",
        )
        .bind(auth_user.tenant_id)
        .fetch_one(pool)
        .await?
    };
    Ok(n)
}

/// Get the Jira base URL for the authenticated user's tenant.
/// Looks up the channels table for a jira-platform channel with enabled = true
/// and extracts base_url from credentials JSON. Returns None if not configured.
pub async fn get_jira_url(pool: &PgPool, auth_user: &AuthUser) -> AppResult<Option<String>> {
    let row: Option<serde_json::Value> = if auth_user.is_super_admin() {
        sqlx::query_scalar(
            r#"SELECT credentials FROM channels
               WHERE platform = 'jira' AND enabled = true
               ORDER BY created_at ASC
               LIMIT 1"#,
        )
        .fetch_optional(pool)
        .await?
    } else {
        sqlx::query_scalar(
            r#"SELECT credentials FROM channels
               WHERE platform = 'jira' AND enabled = true AND tenant_id = $1
               ORDER BY created_at ASC
               LIMIT 1"#,
        )
        .bind(auth_user.tenant_id)
        .fetch_optional(pool)
        .await?
    };

    let url = row
        .as_ref()
        .and_then(|creds| creds.get("base_url"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    Ok(url)
}

/// Create a new approval request.
pub async fn create(
    pool: &PgPool,
    auth_user: &AuthUser,
    req: CreateApprovalRequest,
) -> AppResult<Approval> {
    let approval = sqlx::query_as::<_, Approval>(
        r#"INSERT INTO approvals (command, reason, plan_detail, requested_by, tenant_id, status)
           VALUES ($1, $2, $3, $4, $5, 'pending')
           RETURNING *"#,
    )
    .bind(&req.command)
    .bind(&req.reason)
    .bind(&req.plan_detail)
    .bind(auth_user.user_id)
    .bind(auth_user.tenant_id)
    .fetch_one(pool)
    .await?;

    Ok(approval)
}

/// Update jira_key on an approval.
/// Any user in the same tenant (or super_admin) may update.
pub async fn update(
    pool: &PgPool,
    auth_user: &AuthUser,
    id: Uuid,
    jira_key: Option<String>,
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
    }

    let approval = sqlx::query_as::<_, Approval>(
        r#"UPDATE approvals SET jira_key = $2 WHERE id = $1 RETURNING *"#,
    )
    .bind(id)
    .bind(&jira_key)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Approval not found".to_string()))?;

    Ok(approval)
}

/// Mark the execution result (success/failure).
/// Only the reviewer (reviewed_by) or a super_admin may call this.
pub async fn mark_result(
    pool: &PgPool,
    auth_user: &AuthUser,
    id: Uuid,
    success: bool,
) -> AppResult<Approval> {
    let existing = sqlx::query_as::<_, Approval>("SELECT * FROM approvals WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Approval not found".to_string()))?;

    if !auth_user.is_super_admin() {
        // Only the reviewer can mark result
        let reviewer = existing
            .reviewed_by
            .ok_or_else(|| AppError::BadRequest("Approval has not been reviewed yet".to_string()))?;
        if reviewer != auth_user.user_id {
            return Err(AppError::Forbidden(
                "Only the reviewer can mark the execution result".to_string(),
            ));
        }
    }

    let result_status = if success { "executed" } else { "failed" };
    let execution_result = serde_json::json!({ "success": success });

    let approval = sqlx::query_as::<_, Approval>(
        r#"UPDATE approvals SET
               status = $2,
               execution_result = $3,
               executed_at = NOW(),
               marked_by = $4
           WHERE id = $1
           RETURNING *"#,
    )
    .bind(id)
    .bind(result_status)
    .bind(&execution_result)
    .bind(auth_user.user_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Approval not found".to_string()))?;

    Ok(approval)
}

/// Withdraw a pending approval request.
/// Only the original requester may withdraw, and only while status = 'pending'.
pub async fn withdraw(pool: &PgPool, auth_user: &AuthUser, id: Uuid) -> AppResult<Approval> {
    let existing = sqlx::query_as::<_, Approval>("SELECT * FROM approvals WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Approval not found".to_string()))?;

    // Only the requester may withdraw
    if existing.requested_by != auth_user.user_id {
        return Err(AppError::Forbidden(
            "Only the original requester can withdraw this approval".to_string(),
        ));
    }

    if existing.status != "pending" {
        return Err(AppError::BadRequest(format!(
            "Cannot withdraw an approval with status '{}'",
            existing.status
        )));
    }

    let approval = sqlx::query_as::<_, Approval>(
        r#"UPDATE approvals SET
               status = 'withdrawn',
               withdrawn_at = NOW()
           WHERE id = $1 AND status = 'pending'
           RETURNING *"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Approval not found or already processed".to_string()))?;

    Ok(approval)
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
