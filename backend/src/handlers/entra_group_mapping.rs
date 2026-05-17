use axum::{
    Json,
    extract::{Path, State},
};
use uuid::Uuid;

use crate::AppState;
use crate::error::{AppError, AppResult};
use crate::middleware::auth::AuthUser;
use crate::models::entra_group_mapping::{
    CreateEntraGroupMappingRequest, EntraGroupMapping, UpdateEntraGroupMappingRequest,
};
use crate::services::common::require_non_empty;

/// GET /api/entra-group-mappings
pub async fn list(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
) -> AppResult<Json<Vec<EntraGroupMapping>>> {
    let rows = if auth_user.is_super_admin() {
        sqlx::query_as::<_, EntraGroupMapping>(
            "SELECT * FROM entra_group_mappings ORDER BY group_name",
        )
        .fetch_all(&state.pool)
        .await?
    } else {
        sqlx::query_as::<_, EntraGroupMapping>(
            "SELECT * FROM entra_group_mappings WHERE tenant_id = $1 ORDER BY group_name",
        )
        .bind(auth_user.tenant_id)
        .fetch_all(&state.pool)
        .await?
    };
    Ok(Json(rows))
}

/// POST /api/entra-group-mappings
pub async fn create(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Json(req): Json<CreateEntraGroupMappingRequest>,
) -> AppResult<Json<EntraGroupMapping>> {
    require_non_empty(&req.group_id, "group_id")?;

    let tenant_id = if auth_user.is_super_admin() {
        req.tenant_id
    } else {
        // Non-super-admins can only create mappings for their own tenant
        if req.tenant_id.is_some() && req.tenant_id != auth_user.tenant_id {
            return Err(AppError::Forbidden("Access denied".to_string()));
        }
        auth_user.tenant_id
    };

    let group_name = req.group_name.unwrap_or_default();
    let role = req.role.unwrap_or_else(|| "viewer".to_string());
    let account_access = req.account_access.unwrap_or_else(|| serde_json::json!([]));

    let row = sqlx::query_as::<_, EntraGroupMapping>(
        r#"INSERT INTO entra_group_mappings (group_id, group_name, role, tenant_id, account_access)
           VALUES ($1, $2, $3, $4, $5)
           RETURNING *"#,
    )
    .bind(&req.group_id)
    .bind(&group_name)
    .bind(&role)
    .bind(tenant_id)
    .bind(&account_access)
    .fetch_one(&state.pool)
    .await?;

    Ok(Json(row))
}

/// PUT /api/entra-group-mappings/:id
pub async fn update(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateEntraGroupMappingRequest>,
) -> AppResult<Json<EntraGroupMapping>> {
    let existing = sqlx::query_as::<_, EntraGroupMapping>(
        "SELECT * FROM entra_group_mappings WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Entra group mapping not found".to_string()))?;

    if !auth_user.is_super_admin() && existing.tenant_id != auth_user.tenant_id {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }

    let row = sqlx::query_as::<_, EntraGroupMapping>(
        r#"UPDATE entra_group_mappings SET
           group_id = COALESCE($2, group_id),
           group_name = COALESCE($3, group_name),
           role = COALESCE($4, role),
           tenant_id = COALESCE($5, tenant_id),
           account_access = COALESCE($6, account_access),
           updated_at = NOW()
           WHERE id = $1
           RETURNING *"#,
    )
    .bind(id)
    .bind(&req.group_id)
    .bind(&req.group_name)
    .bind(&req.role)
    .bind(req.tenant_id)
    .bind(&req.account_access)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Entra group mapping not found".to_string()))?;

    Ok(Json(row))
}

/// DELETE /api/entra-group-mappings/:id
pub async fn delete(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let existing = sqlx::query_as::<_, EntraGroupMapping>(
        "SELECT * FROM entra_group_mappings WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Entra group mapping not found".to_string()))?;

    if !auth_user.is_super_admin() && existing.tenant_id != auth_user.tenant_id {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }

    sqlx::query("DELETE FROM entra_group_mappings WHERE id = $1")
        .bind(id)
        .execute(&state.pool)
        .await?;

    Ok(Json(serde_json::json!({"message": "Entra group mapping deleted"})))
}
