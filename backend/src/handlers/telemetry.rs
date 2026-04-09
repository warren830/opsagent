use axum::{
    Json,
    extract::{Path, State},
};
use uuid::Uuid;

use crate::AppState;
use crate::error::{AppError, AppResult};
use crate::middleware::auth::AuthUser;
use crate::models::telemetry::{CreateTelemetryRequest, TelemetryConfig, UpdateTelemetryRequest};

/// GET /api/telemetry — list all configs for the user's tenant
pub async fn list(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
) -> AppResult<Json<Vec<TelemetryConfig>>> {
    let rows = if auth_user.is_super_admin() {
        sqlx::query_as::<_, TelemetryConfig>("SELECT * FROM telemetry_config ORDER BY created_at")
            .fetch_all(&state.pool)
            .await?
    } else {
        sqlx::query_as::<_, TelemetryConfig>("SELECT * FROM telemetry_config WHERE tenant_id = $1 ORDER BY created_at")
            .bind(auth_user.tenant_id)
            .fetch_all(&state.pool)
            .await?
    };
    Ok(Json(rows))
}

/// POST /api/telemetry — create a new config
pub async fn create(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Json(req): Json<CreateTelemetryRequest>,
) -> AppResult<Json<TelemetryConfig>> {
    if req.name.trim().is_empty() {
        return Err(AppError::BadRequest("Name is required".to_string()));
    }

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
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        if let sqlx::Error::Database(ref db_err) = e
            && db_err.constraint().is_some_and(|c| c.contains("tenant_name"))
        {
            return AppError::Conflict(format!("Config '{}' already exists", req.name));
        }
        AppError::Database(e)
    })?;

    Ok(Json(row))
}

/// PUT /api/telemetry/:id — update a config
pub async fn update(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateTelemetryRequest>,
) -> AppResult<Json<TelemetryConfig>> {
    // Verify ownership
    let existing = sqlx::query_as::<_, TelemetryConfig>("SELECT * FROM telemetry_config WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool)
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
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        if let sqlx::Error::Database(ref db_err) = e
            && db_err.constraint().is_some_and(|c| c.contains("tenant_name"))
        {
            return AppError::Conflict("Config name already exists".to_string());
        }
        AppError::Database(e)
    })?;

    Ok(Json(row))
}

/// DELETE /api/telemetry/:id
pub async fn delete(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let existing = sqlx::query_as::<_, TelemetryConfig>("SELECT * FROM telemetry_config WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Telemetry config not found".to_string()))?;

    if !auth_user.is_super_admin() && existing.tenant_id != auth_user.tenant_id {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }

    sqlx::query("DELETE FROM telemetry_config WHERE id = $1")
        .bind(id)
        .execute(&state.pool)
        .await?;

    Ok(Json(serde_json::json!({"message": "Telemetry config deleted"})))
}

/// POST /api/telemetry/test (mock)
pub async fn test_connection(_auth_user: axum::Extension<AuthUser>) -> AppResult<Json<serde_json::Value>> {
    Ok(Json(serde_json::json!({
        "status": "ok",
        "message": "Telemetry connection test successful"
    })))
}
