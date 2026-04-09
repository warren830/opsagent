use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;
use uuid::Uuid;

use crate::AppState;
use crate::error::{AppError, AppResult};
use crate::handlers::account_access::{can_write_account, get_accessible_account_ids};
use crate::middleware::auth::AuthUser;
use crate::models::glossary::{CreateGlossaryRequest, GlossaryEntry, UpdateGlossaryRequest};

#[derive(Debug, Deserialize)]
pub struct GlossaryListQuery {
    pub q: Option<String>,
}

/// GET /api/glossary
/// Returns glossary entries for accounts the user can access
pub async fn list(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Query(query): Query<GlossaryListQuery>,
) -> AppResult<Json<Vec<GlossaryEntry>>> {
    let account_ids = get_accessible_account_ids(&state.pool, &auth_user).await;

    let entries = match &query.q {
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
            .fetch_all(&state.pool)
            .await?
        }
        None => {
            sqlx::query_as::<_, GlossaryEntry>(
                r#"SELECT * FROM glossary
                   WHERE account_id = ANY($1) OR account_id IS NULL
                   ORDER BY term"#,
            )
            .bind(&account_ids)
            .fetch_all(&state.pool)
            .await?
        }
    };

    Ok(Json(entries))
}

/// POST /api/glossary
pub async fn create(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Json(req): Json<CreateGlossaryRequest>,
) -> AppResult<Json<GlossaryEntry>> {
    if req.term.trim().is_empty() {
        return Err(AppError::BadRequest("Term is required".to_string()));
    }

    // Validate write access to the account
    if let Some(account_id) = req.account_id
        && !can_write_account(&state.pool, &auth_user, account_id).await
    {
        return Err(AppError::Forbidden("Read-only access to this account".to_string()));
    }

    // Check if same term already exists
    let existing = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM glossary WHERE LOWER(term) = LOWER($1)")
        .bind(&req.term)
        .fetch_one(&state.pool)
        .await?;

    if existing > 0 {
        return Err(AppError::Conflict(format!("Term '{}' already exists", req.term)));
    }

    // Derive tenant_id from account if provided
    let tenant_id = if let Some(aid) = req.account_id {
        sqlx::query_scalar::<_, Option<Uuid>>("SELECT tenant_id FROM cloud_accounts WHERE id = $1")
            .bind(aid)
            .fetch_optional(&state.pool)
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
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        if let sqlx::Error::Database(ref db_err) = e
            && db_err.constraint().is_some_and(|c| c.starts_with("idx_glossary_term"))
        {
            return AppError::Conflict(format!("Term '{}' already exists", req.term));
        }
        AppError::Database(e)
    })?;

    Ok(Json(entry))
}

/// PUT /api/glossary/:id
pub async fn update(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateGlossaryRequest>,
) -> AppResult<Json<GlossaryEntry>> {
    let existing = sqlx::query_as::<_, GlossaryEntry>("SELECT * FROM glossary WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Glossary entry not found".to_string()))?;

    // Check write access to the account this entry belongs to
    if let Some(aid) = existing.account_id {
        if !can_write_account(&state.pool, &auth_user, aid).await {
            return Err(AppError::Forbidden("Read-only access to this account".to_string()));
        }
    } else if !auth_user.is_admin() {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }

    // Validate write access to new account_id if being changed
    if let Some(new_aid) = req.account_id
        && !can_write_account(&state.pool, &auth_user, new_aid).await
    {
        return Err(AppError::Forbidden("Read-only access to target account".to_string()));
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
    .fetch_optional(&state.pool)
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

    Ok(Json(entry))
}

/// DELETE /api/glossary/:id
pub async fn delete(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let existing = sqlx::query_as::<_, GlossaryEntry>("SELECT * FROM glossary WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Glossary entry not found".to_string()))?;

    if let Some(aid) = existing.account_id {
        if !can_write_account(&state.pool, &auth_user, aid).await {
            return Err(AppError::Forbidden("Read-only access to this account".to_string()));
        }
    } else if !auth_user.is_admin() {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }

    sqlx::query("DELETE FROM glossary WHERE id = $1")
        .bind(id)
        .execute(&state.pool)
        .await?;

    Ok(Json(serde_json::json!({"message": "Glossary entry deleted"})))
}
