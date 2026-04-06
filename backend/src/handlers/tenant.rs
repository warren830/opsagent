use axum::{
    Json,
    extract::{Path, State},
};
use uuid::Uuid;

use crate::AppState;
use crate::error::{AppError, AppResult};
use crate::middleware::auth::AuthUser;
use crate::models::tenant::{CreateTenantRequest, Tenant, UpdateTenantRequest};

/// GET /api/tenants
pub async fn list_tenants(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
) -> AppResult<Json<Vec<Tenant>>> {
    let tenants = if auth_user.is_super_admin() {
        sqlx::query_as::<_, Tenant>("SELECT * FROM tenants ORDER BY name")
            .fetch_all(&state.pool)
            .await?
    } else {
        match auth_user.tenant_id {
            Some(tid) => {
                sqlx::query_as::<_, Tenant>("SELECT * FROM tenants WHERE id = $1")
                    .bind(tid)
                    .fetch_all(&state.pool)
                    .await?
            }
            None => vec![],
        }
    };

    Ok(Json(tenants))
}

/// POST /api/tenants (super_admin only)
pub async fn create_tenant(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Json(req): Json<CreateTenantRequest>,
) -> AppResult<Json<Tenant>> {
    if !auth_user.is_super_admin() {
        return Err(AppError::Forbidden("Only super admins can create tenants".to_string()));
    }

    if req.name.trim().is_empty() || req.slug.trim().is_empty() {
        return Err(AppError::BadRequest("Name and slug are required".to_string()));
    }

    let tenant = sqlx::query_as::<_, Tenant>(
        r#"INSERT INTO tenants (name, slug, aws_account_ids, settings)
           VALUES ($1, $2, $3, $4)
           RETURNING *"#,
    )
    .bind(&req.name)
    .bind(&req.slug)
    .bind(&req.aws_account_ids)
    .bind(&req.settings)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        if let sqlx::Error::Database(ref db_err) = e
            && (db_err.constraint() == Some("tenants_name_key") || db_err.constraint() == Some("tenants_slug_key"))
        {
            return AppError::Conflict("Tenant name or slug already exists".to_string());
        }
        AppError::Database(e)
    })?;

    Ok(Json(tenant))
}

/// GET /api/tenants/:id
pub async fn get_tenant(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Tenant>> {
    if !auth_user.can_access_tenant(&id) {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }

    let tenant = sqlx::query_as::<_, Tenant>("SELECT * FROM tenants WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Tenant not found".to_string()))?;

    Ok(Json(tenant))
}

/// PUT /api/tenants/:id
pub async fn update_tenant(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateTenantRequest>,
) -> AppResult<Json<Tenant>> {
    if !auth_user.is_super_admin() {
        return Err(AppError::Forbidden("Only super admins can update tenants".to_string()));
    }

    let tenant = sqlx::query_as::<_, Tenant>(
        r#"UPDATE tenants SET
           name = COALESCE($2, name),
           slug = COALESCE($3, slug),
           aws_account_ids = COALESCE($4, aws_account_ids),
           settings = COALESCE($5, settings),
           updated_at = NOW()
           WHERE id = $1
           RETURNING *"#,
    )
    .bind(id)
    .bind(&req.name)
    .bind(&req.slug)
    .bind(&req.aws_account_ids)
    .bind(&req.settings)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Tenant not found".to_string()))?;

    Ok(Json(tenant))
}

/// DELETE /api/tenants/:id (super_admin only)
pub async fn delete_tenant(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    if !auth_user.is_super_admin() {
        return Err(AppError::Forbidden("Only super admins can delete tenants".to_string()));
    }

    let result = sqlx::query("DELETE FROM tenants WHERE id = $1")
        .bind(id)
        .execute(&state.pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Tenant not found".to_string()));
    }

    Ok(Json(serde_json::json!({"message": "Tenant deleted"})))
}
