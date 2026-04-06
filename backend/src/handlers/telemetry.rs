use axum::{
    extract::State,
    Json,
};

use crate::error::AppResult;
use crate::middleware::auth::AuthUser;
use crate::models::telemetry::{TelemetryConfig, UpsertTelemetryRequest};
use crate::AppState;

/// GET /api/telemetry
pub async fn get(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
) -> AppResult<Json<Option<TelemetryConfig>>> {
    let row = if auth_user.is_super_admin() {
        // Super admin sees the first config or the one without tenant
        sqlx::query_as::<_, TelemetryConfig>(
            "SELECT * FROM telemetry_config ORDER BY created_at LIMIT 1",
        )
        .fetch_optional(&state.pool)
        .await?
    } else {
        sqlx::query_as::<_, TelemetryConfig>(
            "SELECT * FROM telemetry_config WHERE tenant_id = $1",
        )
        .bind(auth_user.tenant_id)
        .fetch_optional(&state.pool)
        .await?
    };
    Ok(Json(row))
}

/// PUT /api/telemetry (upsert)
pub async fn upsert(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Json(req): Json<UpsertTelemetryRequest>,
) -> AppResult<Json<TelemetryConfig>> {
    let tenant_id = auth_user.tenant_id;

    let row = sqlx::query_as::<_, TelemetryConfig>(
        r#"INSERT INTO telemetry_config (provider, config, enabled, tenant_id)
           VALUES ($1, $2, $3, $4)
           ON CONFLICT (tenant_id) WHERE tenant_id IS NOT NULL
           DO UPDATE SET
             provider = EXCLUDED.provider,
             config = EXCLUDED.config,
             enabled = EXCLUDED.enabled,
             updated_at = NOW()
           RETURNING *"#,
    )
    .bind(&req.provider)
    .bind(&req.config)
    .bind(req.enabled)
    .bind(tenant_id)
    .fetch_one(&state.pool)
    .await?;

    Ok(Json(row))
}

/// POST /api/telemetry/test (mock)
pub async fn test_connection(
    _auth_user: axum::Extension<AuthUser>,
) -> AppResult<Json<serde_json::Value>> {
    Ok(Json(serde_json::json!({
        "status": "ok",
        "message": "Telemetry connection test successful"
    })))
}
