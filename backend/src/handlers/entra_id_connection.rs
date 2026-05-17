use axum::{
    Json,
    extract::{Path, State},
};
use uuid::Uuid;

use crate::AppState;
use crate::error::{AppError, AppResult};
use crate::middleware::auth::AuthUser;
use crate::models::entra_id_connection::{
    CreateEntraIdConnectionRequest, EntraIdConnection, UpdateEntraIdConnectionRequest,
};
use crate::services::common::require_non_empty;

/// GET /api/entra-id-connections
pub async fn list(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
) -> AppResult<Json<Vec<EntraIdConnection>>> {
    let rows = if auth_user.is_super_admin() {
        sqlx::query_as::<_, EntraIdConnection>(
            "SELECT * FROM entra_id_connections ORDER BY name",
        )
        .fetch_all(&state.pool)
        .await?
    } else {
        let tid = auth_user.tenant_id.ok_or_else(|| {
            AppError::Forbidden("No tenant associated with user".to_string())
        })?;
        sqlx::query_as::<_, EntraIdConnection>(
            "SELECT * FROM entra_id_connections WHERE tenant_id = $1 ORDER BY name",
        )
        .bind(tid)
        .fetch_all(&state.pool)
        .await?
    };
    Ok(Json(rows))
}

/// POST /api/entra-id-connections
pub async fn create(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Json(req): Json<CreateEntraIdConnectionRequest>,
) -> AppResult<Json<EntraIdConnection>> {
    if !auth_user.is_super_admin() && auth_user.tenant_id != Some(req.tenant_id) {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }

    require_non_empty(&req.name, "name")?;
    require_non_empty(&req.entra_tenant_id, "entra_tenant_id")?;
    require_non_empty(&req.client_id, "client_id")?;
    require_non_empty(&req.client_secret, "client_secret")?;

    let auto_provision = req.auto_provision.unwrap_or(false);
    let default_role = req.default_role.unwrap_or_else(|| "viewer".to_string());
    let allowed_domains: Vec<String> = req.allowed_domains.unwrap_or_default();

    let row = sqlx::query_as::<_, EntraIdConnection>(
        r#"INSERT INTO entra_id_connections
           (name, entra_tenant_id, client_id, client_secret, tenant_id,
            auto_provision, default_role, enabled, allowed_domains)
           VALUES ($1, $2, $3, $4, $5, $6, $7, true, $8)
           RETURNING *"#,
    )
    .bind(&req.name)
    .bind(&req.entra_tenant_id)
    .bind(&req.client_id)
    .bind(&req.client_secret)
    .bind(req.tenant_id)
    .bind(auto_provision)
    .bind(&default_role)
    .bind(&allowed_domains)
    .fetch_one(&state.pool)
    .await?;

    Ok(Json(row))
}

/// GET /api/entra-id-connections/:id
pub async fn get(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<EntraIdConnection>> {
    let row = sqlx::query_as::<_, EntraIdConnection>(
        "SELECT * FROM entra_id_connections WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Entra ID connection not found".to_string()))?;

    if !auth_user.is_super_admin() && auth_user.tenant_id != Some(row.tenant_id) {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }

    Ok(Json(row))
}

/// PUT /api/entra-id-connections/:id
pub async fn update(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateEntraIdConnectionRequest>,
) -> AppResult<Json<EntraIdConnection>> {
    let existing = sqlx::query_as::<_, EntraIdConnection>(
        "SELECT * FROM entra_id_connections WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Entra ID connection not found".to_string()))?;

    if !auth_user.is_super_admin() && auth_user.tenant_id != Some(existing.tenant_id) {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }

    let row = sqlx::query_as::<_, EntraIdConnection>(
        r#"UPDATE entra_id_connections SET
           name = COALESCE($2, name),
           entra_tenant_id = COALESCE($3, entra_tenant_id),
           client_id = COALESCE($4, client_id),
           client_secret = COALESCE($5, client_secret),
           tenant_id = COALESCE($6, tenant_id),
           auto_provision = COALESCE($7, auto_provision),
           default_role = COALESCE($8, default_role),
           enabled = COALESCE($9, enabled),
           allowed_domains = COALESCE($10, allowed_domains),
           updated_at = NOW()
           WHERE id = $1
           RETURNING *"#,
    )
    .bind(id)
    .bind(&req.name)
    .bind(&req.entra_tenant_id)
    .bind(&req.client_id)
    .bind(&req.client_secret)
    .bind(req.tenant_id)
    .bind(req.auto_provision)
    .bind(&req.default_role)
    .bind(req.enabled)
    .bind(&req.allowed_domains)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Entra ID connection not found".to_string()))?;

    Ok(Json(row))
}

/// DELETE /api/entra-id-connections/:id
pub async fn delete(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let existing = sqlx::query_as::<_, EntraIdConnection>(
        "SELECT * FROM entra_id_connections WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Entra ID connection not found".to_string()))?;

    if !auth_user.is_super_admin() && auth_user.tenant_id != Some(existing.tenant_id) {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }

    sqlx::query("DELETE FROM entra_id_connections WHERE id = $1")
        .bind(id)
        .execute(&state.pool)
        .await?;

    Ok(Json(serde_json::json!({"message": "Entra ID connection deleted"})))
}
