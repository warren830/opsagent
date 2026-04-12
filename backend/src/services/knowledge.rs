use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::middleware::auth::AuthUser;
use crate::models::knowledge::{CreateKnowledgeRequest, KnowledgeFile, UpdateKnowledgeRequest};
use crate::services::account_access::{can_write_account, get_accessible_account_ids};
use crate::services::common::require_non_empty;

/// List knowledge files for accounts the user can access, plus global (account_id IS NULL).
pub async fn list(pool: &PgPool, auth_user: &AuthUser) -> AppResult<Vec<KnowledgeFile>> {
    let account_ids = get_accessible_account_ids(pool, auth_user).await;

    let rows = sqlx::query_as::<_, KnowledgeFile>(
        r#"SELECT * FROM knowledge_files
           WHERE account_id = ANY($1) OR account_id IS NULL
           ORDER BY created_at DESC"#,
    )
    .bind(&account_ids)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

/// Create a new knowledge file.
/// Validates filename, checks write access if account-scoped,
/// derives tenant_id from account or auth_user, computes size_bytes.
pub async fn create(
    pool: &PgPool,
    auth_user: &AuthUser,
    req: CreateKnowledgeRequest,
) -> AppResult<KnowledgeFile> {
    require_non_empty(&req.filename, "Filename")?;

    // Validate write access to the account
    if let Some(account_id) = req.account_id
        && !can_write_account(pool, auth_user, account_id).await
    {
        return Err(AppError::Forbidden(
            "Read-only access to this account".to_string(),
        ));
    }

    // Derive tenant_id from account if provided
    let tenant_id = if let Some(aid) = req.account_id {
        sqlx::query_scalar::<_, Option<Uuid>>("SELECT tenant_id FROM cloud_accounts WHERE id = $1")
            .bind(aid)
            .fetch_optional(pool)
            .await?
            .flatten()
    } else {
        auth_user.tenant_id
    };

    let size_bytes = req.content.len() as i64;

    let row = sqlx::query_as::<_, KnowledgeFile>(
        r#"INSERT INTO knowledge_files (filename, content, size_bytes, mime_type, tenant_id, account_id, created_by)
           VALUES ($1, $2, $3, $4, $5, $6, $7)
           RETURNING *"#,
    )
    .bind(&req.filename)
    .bind(&req.content)
    .bind(size_bytes)
    .bind(&req.mime_type)
    .bind(tenant_id)
    .bind(req.account_id)
    .bind(auth_user.user_id)
    .fetch_one(pool)
    .await?;

    Ok(row)
}

/// Update a knowledge file.
/// Checks write access on the existing account (or admin-only for global files),
/// and validates write access to the new account_id if changing.
pub async fn update(
    pool: &PgPool,
    auth_user: &AuthUser,
    id: Uuid,
    req: UpdateKnowledgeRequest,
) -> AppResult<KnowledgeFile> {
    let existing = sqlx::query_as::<_, KnowledgeFile>("SELECT * FROM knowledge_files WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Knowledge file not found".to_string()))?;

    // Check write access to the account this file belongs to
    if let Some(aid) = existing.account_id {
        if !can_write_account(pool, auth_user, aid).await {
            return Err(AppError::Forbidden(
                "Read-only access to this account".to_string(),
            ));
        }
    } else if !auth_user.is_admin() {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }

    // Validate write access to new account_id if being changed
    if let Some(new_aid) = req.account_id
        && !can_write_account(pool, auth_user, new_aid).await
    {
        return Err(AppError::Forbidden(
            "Read-only access to target account".to_string(),
        ));
    }

    let new_size: Option<i64> = req.content.as_ref().map(|c| c.len() as i64);

    let row = sqlx::query_as::<_, KnowledgeFile>(
        r#"UPDATE knowledge_files SET
           filename = COALESCE($2, filename),
           content = COALESCE($3, content),
           size_bytes = COALESCE($4, size_bytes),
           mime_type = COALESCE($5, mime_type),
           account_id = COALESCE($6, account_id),
           updated_at = NOW()
           WHERE id = $1
           RETURNING *"#,
    )
    .bind(id)
    .bind(&req.filename)
    .bind(&req.content)
    .bind(new_size)
    .bind(&req.mime_type)
    .bind(req.account_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Knowledge file not found".to_string()))?;

    Ok(row)
}

/// Delete a knowledge file.
/// Checks write access on the existing account (or admin-only for global files).
pub async fn delete(pool: &PgPool, auth_user: &AuthUser, id: Uuid) -> AppResult<()> {
    let existing = sqlx::query_as::<_, KnowledgeFile>("SELECT * FROM knowledge_files WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Knowledge file not found".to_string()))?;

    if let Some(aid) = existing.account_id {
        if !can_write_account(pool, auth_user, aid).await {
            return Err(AppError::Forbidden(
                "Read-only access to this account".to_string(),
            ));
        }
    } else if !auth_user.is_admin() {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }

    sqlx::query("DELETE FROM knowledge_files WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;

    Ok(())
}
