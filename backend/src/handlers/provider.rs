use axum::{
    Json,
    extract::{Path, State},
};
use uuid::Uuid;

use crate::AppState;
use crate::error::{AppError, AppResult};
use crate::middleware::auth::AuthUser;
use crate::models::provider::{CreateProviderRequest, Provider, ProviderTypeOption, UpdateProviderRequest};

/// GET /api/providers — list all model configurations for the current tenant
pub async fn list(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
) -> AppResult<Json<Vec<Provider>>> {
    let rows = if auth_user.is_super_admin() {
        sqlx::query_as::<_, Provider>("SELECT * FROM providers ORDER BY is_default DESC, created_at")
            .fetch_all(&state.pool)
            .await?
    } else {
        sqlx::query_as::<_, Provider>(
            "SELECT * FROM providers WHERE tenant_id = $1 ORDER BY is_default DESC, created_at",
        )
        .bind(auth_user.tenant_id)
        .fetch_all(&state.pool)
        .await?
    };
    Ok(Json(rows))
}

/// GET /api/providers/types — available provider types based on environment
pub async fn available_types(State(state): State<AppState>) -> AppResult<Json<Vec<ProviderTypeOption>>> {
    let mut types = Vec::new();

    if state.config.env.is_local() {
        types.push(ProviderTypeOption {
            value: "bedrock".to_string(),
            label: "Amazon Bedrock".to_string(),
        });
    }

    types.push(ProviderTypeOption {
        value: "gateway".to_string(),
        label: "AI Gateway".to_string(),
    });

    Ok(Json(types))
}

/// POST /api/providers — create a new model configuration
pub async fn create(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Json(req): Json<CreateProviderRequest>,
) -> AppResult<Json<Provider>> {
    if !auth_user.is_admin() {
        return Err(AppError::Forbidden("Only admins can configure models".to_string()));
    }

    if req.name.trim().is_empty() {
        return Err(AppError::BadRequest("Name is required".to_string()));
    }

    let tenant_id = auth_user.tenant_id;

    // Check if this is the first provider for the tenant → force is_default = true
    let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM providers WHERE tenant_id IS NOT DISTINCT FROM $1")
        .bind(tenant_id)
        .fetch_one(&state.pool)
        .await?;

    let is_default = if count == 0 { true } else { req.is_default };

    // If setting as default, unset existing defaults
    if is_default {
        sqlx::query("UPDATE providers SET is_default = false WHERE tenant_id IS NOT DISTINCT FROM $1")
            .bind(tenant_id)
            .execute(&state.pool)
            .await?;
    }

    let row = sqlx::query_as::<_, Provider>(
        r#"INSERT INTO providers (name, provider_type, config, is_default, tenant_id)
           VALUES ($1, $2, $3, $4, $5)
           RETURNING *"#,
    )
    .bind(req.name.trim())
    .bind(&req.provider_type)
    .bind(&req.config)
    .bind(is_default)
    .bind(tenant_id)
    .fetch_one(&state.pool)
    .await?;

    Ok(Json(row))
}

/// PUT /api/providers/:id — update a model configuration
pub async fn update(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateProviderRequest>,
) -> AppResult<Json<Provider>> {
    if !auth_user.is_admin() {
        return Err(AppError::Forbidden("Only admins can configure models".to_string()));
    }

    let existing = sqlx::query_as::<_, Provider>("SELECT * FROM providers WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Provider not found".to_string()))?;

    // Verify tenant access
    if !auth_user.is_super_admin() && existing.tenant_id != auth_user.tenant_id {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }

    // If setting as default, unset existing defaults
    if req.is_default == Some(true) {
        sqlx::query("UPDATE providers SET is_default = false WHERE tenant_id IS NOT DISTINCT FROM $1 AND id != $2")
            .bind(existing.tenant_id)
            .bind(id)
            .execute(&state.pool)
            .await?;
    }

    let row = sqlx::query_as::<_, Provider>(
        r#"UPDATE providers SET
           name = COALESCE($2, name),
           provider_type = COALESCE($3, provider_type),
           config = COALESCE($4, config),
           is_default = COALESCE($5, is_default),
           updated_at = NOW()
           WHERE id = $1
           RETURNING *"#,
    )
    .bind(id)
    .bind(&req.name)
    .bind(&req.provider_type)
    .bind(&req.config)
    .bind(req.is_default)
    .fetch_one(&state.pool)
    .await?;

    Ok(Json(row))
}

/// DELETE /api/providers/:id — delete a model configuration
pub async fn delete(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    if !auth_user.is_admin() {
        return Err(AppError::Forbidden("Only admins can configure models".to_string()));
    }

    let existing = sqlx::query_as::<_, Provider>("SELECT * FROM providers WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Provider not found".to_string()))?;

    if !auth_user.is_super_admin() && existing.tenant_id != auth_user.tenant_id {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }

    sqlx::query("DELETE FROM providers WHERE id = $1")
        .bind(id)
        .execute(&state.pool)
        .await?;

    // If deleted was default, promote the next one
    if existing.is_default {
        sqlx::query(
            r#"UPDATE providers SET is_default = true
               WHERE id = (
                   SELECT id FROM providers
                   WHERE tenant_id IS NOT DISTINCT FROM $1
                   ORDER BY created_at ASC LIMIT 1
               )"#,
        )
        .bind(existing.tenant_id)
        .execute(&state.pool)
        .await
        .ok(); // best-effort
    }

    Ok(Json(serde_json::json!({"message": "Provider deleted"})))
}
