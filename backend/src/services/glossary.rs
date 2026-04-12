use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::services::account_access::{can_write_account, get_accessible_account_ids};
use crate::middleware::auth::AuthUser;
use crate::models::glossary::{CreateGlossaryRequest, GlossaryEntry, UpdateGlossaryRequest};

/// List glossary entries visible to the authenticated user.
/// Entries are visible if their account_id is in the user's accessible accounts or is NULL.
/// Optionally filters by a search term (case-insensitive LIKE on term/full_name/description).
pub async fn list(
    pool: &PgPool,
    auth_user: &AuthUser,
    search: Option<&str>,
) -> AppResult<Vec<GlossaryEntry>> {
    let account_ids = get_accessible_account_ids(pool, auth_user).await;

    let entries = match search {
        Some(q) => {
            let pattern = format!("%{}%", q.to_lowercase());
            sqlx::query_as::<_, GlossaryEntry>(
                r#"SELECT * FROM glossary
                   WHERE (account_id = ANY($1) OR account_id IS NULL)
                     AND (LOWER(term) LIKE $2
                          OR LOWER(COALESCE(full_name, '')) LIKE $2
                          OR LOWER(COALESCE(description, '')) LIKE $2)
                   ORDER BY term"#,
            )
            .bind(&account_ids)
            .bind(&pattern)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query_as::<_, GlossaryEntry>(
                r#"SELECT * FROM glossary
                   WHERE account_id = ANY($1) OR account_id IS NULL
                   ORDER BY term"#,
            )
            .bind(&account_ids)
            .fetch_all(pool)
            .await?
        }
    };

    Ok(entries)
}

/// Create a new glossary entry.
/// Validates term is non-empty and checks write access to the account if provided.
/// Derives tenant_id from the account or falls back to the user's tenant_id.
pub async fn create(
    pool: &PgPool,
    auth_user: &AuthUser,
    req: CreateGlossaryRequest,
) -> AppResult<GlossaryEntry> {
    if req.term.trim().is_empty() {
        return Err(AppError::BadRequest("Term is required".to_string()));
    }

    // Validate write access to the account
    if let Some(account_id) = req.account_id
        && !can_write_account(pool, auth_user, account_id).await
    {
        return Err(AppError::Forbidden(
            "Read-only access to this account".to_string(),
        ));
    }

    // Check if same term already exists (case-insensitive)
    let existing =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM glossary WHERE LOWER(term) = LOWER($1)")
            .bind(&req.term)
            .fetch_one(pool)
            .await?;

    if existing > 0 {
        return Err(AppError::Conflict(format!(
            "Term '{}' already exists",
            req.term
        )));
    }

    // Derive tenant_id from account if provided
    let tenant_id = if let Some(aid) = req.account_id {
        sqlx::query_scalar::<_, Option<Uuid>>(
            "SELECT tenant_id FROM cloud_accounts WHERE id = $1",
        )
        .bind(aid)
        .fetch_optional(pool)
        .await?
        .flatten()
    } else {
        auth_user.tenant_id
    };

    let entry = sqlx::query_as::<_, GlossaryEntry>(
        r#"INSERT INTO glossary (term, full_name, description, aliases, aws_accounts, services, tenant_id, account_id)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
           RETURNING *"#,
    )
    .bind(&req.term)
    .bind(&req.full_name)
    .bind(&req.description)
    .bind(&req.aliases)
    .bind(&req.aws_accounts)
    .bind(&req.services)
    .bind(tenant_id)
    .bind(req.account_id)
    .fetch_one(pool)
    .await
    .map_err(|e| {
        if let sqlx::Error::Database(ref db_err) = e
            && db_err.constraint().is_some_and(|c| c.starts_with("idx_glossary_term"))
        {
            return AppError::Conflict(format!("Term '{}' already exists", &req.term));
        }
        AppError::Database(e)
    })?;

    Ok(entry)
}

/// Update an existing glossary entry.
/// Checks write access based on the entry's account_id (or requires admin for global entries).
/// If the account_id is being changed, also checks write access to the new account.
pub async fn update(
    pool: &PgPool,
    auth_user: &AuthUser,
    id: Uuid,
    req: UpdateGlossaryRequest,
) -> AppResult<GlossaryEntry> {
    let existing = sqlx::query_as::<_, GlossaryEntry>("SELECT * FROM glossary WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Glossary entry not found".to_string()))?;

    // Check write access to the account this entry belongs to
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

    let entry = sqlx::query_as::<_, GlossaryEntry>(
        r#"UPDATE glossary SET
           term = COALESCE($2, term),
           full_name = COALESCE($3, full_name),
           description = COALESCE($4, description),
           aliases = COALESCE($5, aliases),
           aws_accounts = COALESCE($6, aws_accounts),
           services = COALESCE($7, services),
           account_id = COALESCE($8, account_id),
           updated_at = NOW()
           WHERE id = $1
           RETURNING *"#,
    )
    .bind(id)
    .bind(&req.term)
    .bind(&req.full_name)
    .bind(&req.description)
    .bind(&req.aliases)
    .bind(&req.aws_accounts)
    .bind(&req.services)
    .bind(req.account_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        if let sqlx::Error::Database(ref db_err) = e
            && db_err.constraint().is_some_and(|c| c.starts_with("idx_glossary_term"))
        {
            return AppError::Conflict("Term already exists".to_string());
        }
        AppError::Database(e)
    })?
    .ok_or_else(|| AppError::NotFound("Glossary entry not found".to_string()))?;

    Ok(entry)
}

/// Delete a glossary entry.
/// Checks write access based on the entry's account_id (or requires admin for global entries).
pub async fn delete(pool: &PgPool, auth_user: &AuthUser, id: Uuid) -> AppResult<()> {
    let existing = sqlx::query_as::<_, GlossaryEntry>("SELECT * FROM glossary WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Glossary entry not found".to_string()))?;

    if let Some(aid) = existing.account_id {
        if !can_write_account(pool, auth_user, aid).await {
            return Err(AppError::Forbidden(
                "Read-only access to this account".to_string(),
            ));
        }
    } else if !auth_user.is_admin() {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }

    sqlx::query("DELETE FROM glossary WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;

    Ok(())
}
