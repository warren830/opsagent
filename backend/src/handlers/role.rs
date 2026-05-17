use axum::{
    Json,
    extract::{Path, State},
};
use sqlx::PgPool;
use uuid::Uuid;

use crate::AppState;
use crate::error::{AppError, AppResult};
use crate::middleware::auth::AuthUser;
use crate::models::role::{CreateRoleRequest, Role, UpdateRoleRequest};
use crate::services::common::{require_non_empty, require_super_admin};

// TODO: add role_cache (HashMap<String, HashSet<String>>) to AppState for
// permission lookups without hitting the DB on every request.

async fn fetch_role(pool: &PgPool, id: Uuid) -> AppResult<Role> {
    sqlx::query_as::<_, Role>("SELECT * FROM roles WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Role not found".to_string()))
}

/// GET /api/roles
pub async fn list(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
) -> AppResult<Json<Vec<Role>>> {
    let rows = if auth_user.is_super_admin() {
        sqlx::query_as::<_, Role>("SELECT * FROM roles ORDER BY name")
            .fetch_all(&state.pool)
            .await?
    } else {
        sqlx::query_as::<_, Role>(
            "SELECT * FROM roles WHERE tenant_id = $1 OR tenant_id IS NULL ORDER BY name",
        )
        .bind(auth_user.tenant_id)
        .fetch_all(&state.pool)
        .await?
    };
    Ok(Json(rows))
}

/// POST /api/roles
pub async fn create(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Json(req): Json<CreateRoleRequest>,
) -> AppResult<Json<Role>> {
    require_super_admin(&auth_user, "create roles")?;
    require_non_empty(&req.name, "name")?;
    require_non_empty(&req.label, "label")?;

    let row = sqlx::query_as::<_, Role>(
        r#"INSERT INTO roles (name, label, description, permissions, is_system, tenant_id)
           VALUES ($1, $2, $3, $4, $5, $6)
           RETURNING *"#,
    )
    .bind(&req.name)
    .bind(&req.label)
    .bind(&req.description)
    .bind(&req.permissions)
    .bind(req.is_system)
    .bind(auth_user.tenant_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        if let sqlx::Error::Database(ref db_err) = e {
            if db_err.constraint() == Some("roles_name_key") {
                return AppError::Conflict("Role name already exists".to_string());
            }
        }
        AppError::Database(e)
    })?;

    Ok(Json(row))
}

/// PUT /api/roles/:id
pub async fn update(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateRoleRequest>,
) -> AppResult<Json<Role>> {
    require_super_admin(&auth_user, "update roles")?;

    let existing = fetch_role(&state.pool, id).await?;
    if existing.is_system {
        return Err(AppError::Forbidden("System roles cannot be modified".to_string()));
    }

    let row = sqlx::query_as::<_, Role>(
        r#"UPDATE roles SET
           name = COALESCE($2, name),
           label = COALESCE($3, label),
           description = COALESCE($4, description),
           permissions = COALESCE($5, permissions),
           is_system = COALESCE($6, is_system),
           updated_at = NOW()
           WHERE id = $1
           RETURNING *"#,
    )
    .bind(id)
    .bind(&req.name)
    .bind(&req.label)
    .bind(&req.description)
    .bind(&req.permissions)
    .bind(req.is_system)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Role not found".to_string()))?;

    Ok(Json(row))
}

/// DELETE /api/roles/:id
pub async fn delete(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    require_super_admin(&auth_user, "delete roles")?;

    let existing = fetch_role(&state.pool, id).await?;
    if existing.is_system {
        return Err(AppError::Forbidden("System roles cannot be deleted".to_string()));
    }

    let result = sqlx::query("DELETE FROM roles WHERE id = $1")
        .bind(id)
        .execute(&state.pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Role not found".to_string()));
    }

    Ok(Json(serde_json::json!({"message": "Role deleted"})))
}
