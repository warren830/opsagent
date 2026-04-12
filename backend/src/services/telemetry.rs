use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::middleware::auth::AuthUser;
use crate::models::telemetry::{CreateTelemetryRequest, TelemetryConfig, UpdateTelemetryRequest};
use crate::services::common::require_non_empty;

/// List all telemetry configs for the current tenant.
/// Super admins see all configs; other users see only their tenant's.
pub async fn list(pool: &PgPool, auth_user: &AuthUser) -> AppResult<Vec<TelemetryConfig>> {
    let rows = if auth_user.is_super_admin() {
        sqlx::query_as::<_, TelemetryConfig>(
            "SELECT * FROM telemetry_config ORDER BY created_at",
        )
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as::<_, TelemetryConfig>(
            "SELECT * FROM telemetry_config WHERE tenant_id = $1 ORDER BY created_at",
        )
        .bind(auth_user.tenant_id)
        .fetch_all(pool)
        .await?
    };
    Ok(rows)
}

/// Create a new telemetry config.
/// Validates name is non-empty. Maps tenant_name constraint to Conflict error.
pub async fn create(
    pool: &PgPool,
    auth_user: &AuthUser,
    req: CreateTelemetryRequest,
) -> AppResult<TelemetryConfig> {
    require_non_empty(&req.name, "Name")?;

    let tenant_id = auth_user.tenant_id;

    let row = sqlx::query_as::<_, TelemetryConfig>(
        r#"INSERT INTO telemetry_config (name, provider, config, routing, enabled, tenant_id)
           VALUES ($1, $2, $3, $4, $5, $6)
           RETURNING *"#,
    )
    .bind(&req.name)
    .bind(&req.provider)
    .bind(&req.config)
    .bind(&req.routing)
    .bind(req.enabled)
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .map_err(|e| {
        if let sqlx::Error::Database(ref db_err) = e
            && db_err.constraint().is_some_and(|c| c.contains("tenant_name"))
        {
            return AppError::Conflict(format!("Config '{}' already exists", req.name));
        }
        AppError::Database(e)
    })?;

    Ok(row)
}

/// Update an existing telemetry config.
/// Verifies tenant ownership for non-super_admin users.
pub async fn update(
    pool: &PgPool,
    auth_user: &AuthUser,
    id: Uuid,
    req: UpdateTelemetryRequest,
) -> AppResult<TelemetryConfig> {
    // Verify ownership
    let existing = sqlx::query_as::<_, TelemetryConfig>(
        "SELECT * FROM telemetry_config WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Telemetry config not found".to_string()))?;

    if !auth_user.is_super_admin() && existing.tenant_id != auth_user.tenant_id {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }

    let row = sqlx::query_as::<_, TelemetryConfig>(
        r#"UPDATE telemetry_config SET
           name = COALESCE($2, name),
           provider = COALESCE($3, provider),
           config = COALESCE($4, config),
           routing = COALESCE($5, routing),
           enabled = COALESCE($6, enabled),
           updated_at = NOW()
           WHERE id = $1
           RETURNING *"#,
    )
    .bind(id)
    .bind(&req.name)
    .bind(&req.provider)
    .bind(&req.config)
    .bind(&req.routing)
    .bind(req.enabled)
    .fetch_one(pool)
    .await
    .map_err(|e| {
        if let sqlx::Error::Database(ref db_err) = e
            && db_err.constraint().is_some_and(|c| c.contains("tenant_name"))
        {
            return AppError::Conflict("Config name already exists".to_string());
        }
        AppError::Database(e)
    })?;

    Ok(row)
}

/// Delete a telemetry config by ID.
/// Verifies tenant ownership for non-super_admin users.
pub async fn delete(pool: &PgPool, auth_user: &AuthUser, id: Uuid) -> AppResult<()> {
    let existing = sqlx::query_as::<_, TelemetryConfig>(
        "SELECT * FROM telemetry_config WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Telemetry config not found".to_string()))?;

    if !auth_user.is_super_admin() && existing.tenant_id != auth_user.tenant_id {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }

    sqlx::query("DELETE FROM telemetry_config WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;

    Ok(())
}
