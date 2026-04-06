use axum::{
    Json,
    extract::{Path, State},
};
use uuid::Uuid;

use crate::AppState;
use crate::error::{AppError, AppResult};
use crate::handlers::account_access::{can_write_account, get_accessible_account_ids};
use crate::middleware::auth::AuthUser;
use crate::models::knowledge::{CreateKnowledgeRequest, KnowledgeFile, UpdateKnowledgeRequest};

/// GET /api/knowledge
/// Returns knowledge files for accounts the user can access
pub async fn list(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
) -> AppResult<Json<Vec<KnowledgeFile>>> {
    let account_ids = get_accessible_account_ids(&state.pool, &auth_user).await;

    let rows = sqlx::query_as::<_, KnowledgeFile>(
        r#"SELECT * FROM knowledge_files
           WHERE account_id = ANY($1) OR account_id IS NULL
           ORDER BY created_at DESC"#,
    )
    .bind(&account_ids)
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(rows))
}

/// POST /api/knowledge
pub async fn create(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Json(req): Json<CreateKnowledgeRequest>,
) -> AppResult<Json<KnowledgeFile>> {
    if req.filename.trim().is_empty() {
        return Err(AppError::BadRequest("Filename is required".to_string()));
    }

    let visibility = match req.visibility.as_str() {
        "public" | "private" => req.visibility.clone(),
        _ => "public".to_string(),
    };

    // Validate write access to the account
    if let Some(account_id) = req.account_id
        && !can_write_account(&state.pool, &auth_user, account_id).await
    {
        return Err(AppError::Forbidden("Read-only access to this account".to_string()));
    }

    let user_id = if visibility == "private" {
        Some(auth_user.user_id)
    } else {
        None
    };

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

    let size_bytes = req.content.len() as i64;

    let row = sqlx::query_as::<_, KnowledgeFile>(
        r#"INSERT INTO knowledge_files (filename, content, size_bytes, mime_type, tenant_id, user_id, account_id, created_by, visibility)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
           RETURNING *"#,
    )
    .bind(&req.filename)
    .bind(&req.content)
    .bind(size_bytes)
    .bind(&req.mime_type)
    .bind(tenant_id)
    .bind(user_id)
    .bind(req.account_id)
    .bind(auth_user.user_id)
    .bind(&visibility)
    .fetch_one(&state.pool)
    .await?;

    Ok(Json(row))
}

/// PUT /api/knowledge/:id
pub async fn update(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateKnowledgeRequest>,
) -> AppResult<Json<KnowledgeFile>> {
    let existing = sqlx::query_as::<_, KnowledgeFile>("SELECT * FROM knowledge_files WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Knowledge file not found".to_string()))?;

    // Check write access to the account this file belongs to
    if let Some(aid) = existing.account_id {
        if !can_write_account(&state.pool, &auth_user, aid).await {
            return Err(AppError::Forbidden("Read-only access to this account".to_string()));
        }
    } else if !auth_user.is_admin() && existing.user_id != Some(auth_user.user_id) {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }

    // Validate write access to new account_id if being changed
    if let Some(new_aid) = req.account_id
        && !can_write_account(&state.pool, &auth_user, new_aid).await
    {
        return Err(AppError::Forbidden("Read-only access to target account".to_string()));
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
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Knowledge file not found".to_string()))?;

    Ok(Json(row))
}

/// DELETE /api/knowledge/:id
pub async fn delete(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let existing = sqlx::query_as::<_, KnowledgeFile>("SELECT * FROM knowledge_files WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Knowledge file not found".to_string()))?;

    if let Some(aid) = existing.account_id {
        if !can_write_account(&state.pool, &auth_user, aid).await {
            return Err(AppError::Forbidden("Read-only access to this account".to_string()));
        }
    } else if !auth_user.is_admin() && existing.user_id != Some(auth_user.user_id) {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }

    sqlx::query("DELETE FROM knowledge_files WHERE id = $1")
        .bind(id)
        .execute(&state.pool)
        .await?;

    Ok(Json(serde_json::json!({"message": "Knowledge file deleted"})))
}
