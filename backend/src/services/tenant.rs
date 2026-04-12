use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::middleware::auth::AuthUser;
use crate::models::tenant::{CreateTenantRequest, Tenant, UpdateTenantRequest};
use crate::services::common::{map_constraint_error, require_non_empty, require_super_admin, tenant_filter};

const CONSTRAINT_MAPPINGS: &[(&str, &str)] = &[
    ("tenants_name_key", "Tenant name or slug already exists"),
    ("tenants_slug_key", "Tenant name or slug already exists"),
];

/// List tenants visible to the authenticated user.
/// Super admins see all tenants; members see only their own.
pub async fn list(pool: &PgPool, auth_user: &AuthUser) -> AppResult<Vec<Tenant>> {
    let tenants = match tenant_filter(auth_user) {
        None => {
            sqlx::query_as::<_, Tenant>("SELECT * FROM tenants ORDER BY name")
                .fetch_all(pool)
                .await?
        }
        Some(tid) => {
            sqlx::query_as::<_, Tenant>("SELECT * FROM tenants WHERE id = $1")
                .bind(tid)
                .fetch_all(pool)
                .await?
        }
    };
    Ok(tenants)
}

/// Get a single tenant by ID (with access check).
pub async fn get(pool: &PgPool, auth_user: &AuthUser, id: Uuid) -> AppResult<Tenant> {
    if !auth_user.can_access_tenant(&id) {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }

    sqlx::query_as::<_, Tenant>("SELECT * FROM tenants WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Tenant not found".to_string()))
}

/// Create a new tenant (super_admin only).
pub async fn create(
    pool: &PgPool,
    auth_user: &AuthUser,
    req: CreateTenantRequest,
) -> AppResult<Tenant> {
    require_super_admin(auth_user, "create tenants")?;
    require_non_empty(&req.name, "name")?;
    require_non_empty(&req.slug, "slug")?;

    let tenant = sqlx::query_as::<_, Tenant>(
        r#"INSERT INTO tenants (name, slug, aws_account_ids, settings)
           VALUES ($1, $2, $3, $4)
           RETURNING *"#,
    )
    .bind(&req.name)
    .bind(&req.slug)
    .bind(&req.aws_account_ids)
    .bind(&req.settings)
    .fetch_one(pool)
    .await
    .map_err(|e| map_constraint_error(e, CONSTRAINT_MAPPINGS))?;

    Ok(tenant)
}

/// Update an existing tenant (super_admin only).
pub async fn update(
    pool: &PgPool,
    auth_user: &AuthUser,
    id: Uuid,
    req: UpdateTenantRequest,
) -> AppResult<Tenant> {
    require_super_admin(auth_user, "update tenants")?;

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
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Tenant not found".to_string()))?;

    Ok(tenant)
}

/// Delete a tenant by ID (super_admin only).
pub async fn delete(pool: &PgPool, auth_user: &AuthUser, id: Uuid) -> AppResult<()> {
    require_super_admin(auth_user, "delete tenants")?;

    let result = sqlx::query("DELETE FROM tenants WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Tenant not found".to_string()));
    }

    Ok(())
}
