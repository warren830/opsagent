use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::middleware::auth::AuthUser;
use crate::models::channel::{Channel, CreateChannelRequest, UpdateChannelRequest};
use crate::services::common::require_non_empty;

/// List channels visible to the authenticated user.
/// Super admins see all channels; other users see only their tenant's channels.
pub async fn list(pool: &PgPool, auth_user: &AuthUser) -> AppResult<Vec<Channel>> {
    let rows = if auth_user.is_super_admin() {
        sqlx::query_as::<_, Channel>("SELECT * FROM channels ORDER BY name")
            .fetch_all(pool)
            .await?
    } else {
        sqlx::query_as::<_, Channel>(
            "SELECT * FROM channels WHERE tenant_id = $1 ORDER BY name",
        )
        .bind(auth_user.tenant_id)
        .fetch_all(pool)
        .await?
    };
    Ok(rows)
}

/// Create a new channel. Validates that name and platform are non-empty.
/// The channel is associated with the authenticated user's tenant.
pub async fn create(
    pool: &PgPool,
    auth_user: &AuthUser,
    req: CreateChannelRequest,
) -> AppResult<Channel> {
    require_non_empty(&req.name, "Name")?;
    require_non_empty(&req.platform, "Platform")?;

    let row = sqlx::query_as::<_, Channel>(
        r#"INSERT INTO channels (platform, name, credentials, settings, enabled, tenant_id)
           VALUES ($1, $2, $3, $4, $5, $6)
           RETURNING *"#,
    )
    .bind(&req.platform)
    .bind(&req.name)
    .bind(&req.credentials)
    .bind(&req.settings)
    .bind(req.enabled)
    .bind(auth_user.tenant_id)
    .fetch_one(pool)
    .await?;

    Ok(row)
}

/// Update an existing channel. Non-admin users can only update channels
/// belonging to their own tenant.
pub async fn update(
    pool: &PgPool,
    auth_user: &AuthUser,
    id: Uuid,
    req: UpdateChannelRequest,
) -> AppResult<Channel> {
    // If not super_admin, verify the channel belongs to user's tenant
    if !auth_user.is_super_admin() {
        let existing = sqlx::query_as::<_, Channel>("SELECT * FROM channels WHERE id = $1")
            .bind(id)
            .fetch_optional(pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Channel not found".to_string()))?;
        if existing.tenant_id != auth_user.tenant_id {
            return Err(AppError::Forbidden("Access denied".to_string()));
        }
    }

    let row = sqlx::query_as::<_, Channel>(
        r#"UPDATE channels SET
           platform = COALESCE($2, platform),
           name = COALESCE($3, name),
           credentials = COALESCE($4, credentials),
           settings = COALESCE($5, settings),
           enabled = COALESCE($6, enabled),
           updated_at = NOW()
           WHERE id = $1
           RETURNING *"#,
    )
    .bind(id)
    .bind(&req.platform)
    .bind(&req.name)
    .bind(&req.credentials)
    .bind(&req.settings)
    .bind(req.enabled)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Channel not found".to_string()))?;

    Ok(row)
}

/// Delete a channel by ID. Non-admin users can only delete channels
/// belonging to their own tenant.
pub async fn delete(pool: &PgPool, auth_user: &AuthUser, id: Uuid) -> AppResult<()> {
    // If not super_admin, verify the channel belongs to user's tenant
    if !auth_user.is_super_admin() {
        let existing = sqlx::query_as::<_, Channel>("SELECT * FROM channels WHERE id = $1")
            .bind(id)
            .fetch_optional(pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Channel not found".to_string()))?;
        if existing.tenant_id != auth_user.tenant_id {
            return Err(AppError::Forbidden("Access denied".to_string()));
        }
    }

    let result = sqlx::query("DELETE FROM channels WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Channel not found".to_string()));
    }

    Ok(())
}
