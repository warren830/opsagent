use axum::{
    Json,
    extract::{Path, State},
};
use serde::{Deserialize, Serialize};
use std::time::Instant;
use uuid::Uuid;

use crate::AppState;
use crate::error::{AppError, AppResult};
use crate::middleware::auth::AuthUser;
use crate::models::provider::{CreateProviderRequest, Provider, ProviderTypeOption, UpdateProviderRequest};
use crate::services;

/// GET /api/providers — list all model configurations for the current tenant
pub async fn list(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
) -> AppResult<Json<Vec<Provider>>> {
    let rows = services::provider::list(&state.pool, &auth_user).await?;
    Ok(Json(rows))
}

/// GET /api/providers/types — available provider types based on environment
pub async fn available_types(State(state): State<AppState>) -> AppResult<Json<Vec<ProviderTypeOption>>> {
    let types = services::provider::available_types(state.config.env.is_local());
    Ok(Json(types))
}

/// POST /api/providers — create a new model configuration
pub async fn create(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Json(req): Json<CreateProviderRequest>,
) -> AppResult<Json<Provider>> {
    let row = services::provider::create(&state.pool, &auth_user, req).await?;
    Ok(Json(row))
}

/// PUT /api/providers/:id — update a model configuration
pub async fn update(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateProviderRequest>,
) -> AppResult<Json<Provider>> {
    let row = services::provider::update(&state.pool, &auth_user, id, req).await?;
    Ok(Json(row))
}

/// DELETE /api/providers/:id — delete a model configuration
pub async fn delete(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    services::provider::delete(&state.pool, &auth_user, id).await?;
    Ok(Json(serde_json::json!({"message": "Provider deleted"})))
}

// ---------------------------------------------------------------------------
// Assignment + test endpoints
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct AssignmentCount {
    pub provider_id: Uuid,
    pub count: i64,
}

/// GET /api/providers/assignments — how many tenants each provider is assigned to
pub async fn tenant_assignment_counts(
    _auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
) -> AppResult<Json<Vec<AssignmentCount>>> {
    let rows = sqlx::query_as::<_, (Uuid, i64)>(
        "SELECT provider_id, COUNT(*) as count FROM provider_tenant_assignments GROUP BY provider_id",
    )
    .fetch_all(&state.pool)
    .await?;

    let result = rows
        .into_iter()
        .map(|(provider_id, count)| AssignmentCount { provider_id, count })
        .collect();

    Ok(Json(result))
}

// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct TestConnectionResponse {
    pub ok: bool,
    pub latency_ms: u64,
    pub error: Option<String>,
}

/// POST /api/providers/:id/test — probe the provider endpoint
pub async fn test_connection(
    _auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<TestConnectionResponse>> {
    let provider = sqlx::query_as::<_, Provider>("SELECT * FROM providers WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Provider not found".to_string()))?;

    // Extract base_url from config; fall back to OpenAI default
    let base_url = provider
        .config
        .get("base_url")
        .and_then(|v| v.as_str())
        .unwrap_or("https://api.openai.com")
        .trim_end_matches('/')
        .to_string();

    // Build probe URL — try /v1/models (OpenAI-compatible) otherwise root
    let probe_url = if base_url.contains("openai") || base_url.contains("gateway") {
        format!("{}/v1/models", base_url)
    } else {
        base_url.clone()
    };

    let start = Instant::now();

    // Optionally attach API key header if present in config
    let mut req_builder = state
        .http_client
        .get(&probe_url)
        .timeout(std::time::Duration::from_secs(5));

    if let Some(api_key) = provider.config.get("api_key").and_then(|v| v.as_str()) {
        req_builder = req_builder.header("Authorization", format!("Bearer {}", api_key));
    }

    let result = req_builder.send().await;
    let latency_ms = start.elapsed().as_millis() as u64;

    let resp = match result {
        Err(e) => {
            return Ok(Json(TestConnectionResponse {
                ok: false,
                latency_ms,
                error: Some(e.to_string()),
            }));
        }
        Ok(r) => r,
    };

    // Treat 2xx and 401 as "reachable" — 401 means the key is wrong but the
    // endpoint is up; anything else (5xx, connection refused, etc.) is a failure.
    let status = resp.status();
    if status.is_success() || status == 401 {
        Ok(Json(TestConnectionResponse {
            ok: true,
            latency_ms,
            error: None,
        }))
    } else {
        Ok(Json(TestConnectionResponse {
            ok: false,
            latency_ms,
            error: Some(format!("HTTP {}", status.as_u16())),
        }))
    }
}

// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct ProviderTenant {
    pub tenant_id: Uuid,
    pub tenant_name: String,
    pub is_default: bool,
}

/// GET /api/providers/:id/tenants — which tenants this provider is assigned to
pub async fn list_provider_tenants(
    _auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Vec<ProviderTenant>>> {
    let rows = sqlx::query_as::<_, (Uuid, String, bool)>(
        r#"SELECT t.id, t.name, pta.is_default
           FROM provider_tenant_assignments pta
           JOIN tenants t ON t.id = pta.tenant_id
           WHERE pta.provider_id = $1
           ORDER BY t.name"#,
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await?;

    let result = rows
        .into_iter()
        .map(|(tenant_id, tenant_name, is_default)| ProviderTenant {
            tenant_id,
            tenant_name,
            is_default,
        })
        .collect();

    Ok(Json(result))
}

// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct AssignTenantsRequest {
    pub tenant_ids: Vec<Uuid>,
}

/// PUT /api/providers/:id/tenants — replace the full tenant assignment list for a provider
pub async fn assign_provider_tenants(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<AssignTenantsRequest>,
) -> AppResult<Json<serde_json::Value>> {
    if !auth_user.is_super_admin() {
        return Err(AppError::Forbidden(
            "Only super_admin can manage provider assignments".to_string(),
        ));
    }

    // Verify provider exists
    let exists = sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM providers WHERE id = $1)")
        .bind(id)
        .fetch_one(&state.pool)
        .await?;
    if !exists {
        return Err(AppError::NotFound("Provider not found".to_string()));
    }

    let mut tx = state.pool.begin().await?;

    // Delete existing assignments for this provider
    sqlx::query("DELETE FROM provider_tenant_assignments WHERE provider_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    // Insert new assignments; first tenant gets is_default=true if list is non-empty
    for (i, tenant_id) in req.tenant_ids.iter().enumerate() {
        sqlx::query(
            r#"INSERT INTO provider_tenant_assignments (provider_id, tenant_id, is_default)
               VALUES ($1, $2, $3)
               ON CONFLICT (provider_id, tenant_id) DO NOTHING"#,
        )
        .bind(id)
        .bind(tenant_id)
        .bind(i == 0) // first in list becomes default
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    Ok(Json(serde_json::json!({
        "message": "Provider tenant assignments updated",
        "provider_id": id,
        "assigned_count": req.tenant_ids.len()
    })))
}

// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct TenantProvider {
    pub provider_id: Uuid,
    pub provider_name: String,
    pub provider_type: String,
    pub is_default: bool,
}

/// GET /api/tenants/:id/providers — providers assigned to a tenant
pub async fn list_tenant_providers(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(tenant_id): Path<Uuid>,
) -> AppResult<Json<Vec<TenantProvider>>> {
    // Must be super_admin or the tenant's own admin
    if !auth_user.is_super_admin() && auth_user.tenant_id.as_ref() != Some(&tenant_id) {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }

    let rows = sqlx::query_as::<_, (Uuid, String, String, bool)>(
        r#"SELECT p.id, p.name, p.provider_type, pta.is_default
           FROM provider_tenant_assignments pta
           JOIN providers p ON p.id = pta.provider_id
           WHERE pta.tenant_id = $1
           ORDER BY pta.is_default DESC, p.name"#,
    )
    .bind(tenant_id)
    .fetch_all(&state.pool)
    .await?;

    let result = rows
        .into_iter()
        .map(|(provider_id, provider_name, provider_type, is_default)| TenantProvider {
            provider_id,
            provider_name,
            provider_type,
            is_default,
        })
        .collect();

    Ok(Json(result))
}

// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct AssignProvidersRequest {
    pub provider_ids: Vec<Uuid>,
}

/// PUT /api/tenants/:id/providers — replace the full provider list for a tenant
pub async fn assign_tenant_providers(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(tenant_id): Path<Uuid>,
    Json(req): Json<AssignProvidersRequest>,
) -> AppResult<Json<serde_json::Value>> {
    if !auth_user.is_super_admin() {
        return Err(AppError::Forbidden(
            "Only super_admin can manage tenant provider assignments".to_string(),
        ));
    }

    // Verify tenant exists
    let exists = sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM tenants WHERE id = $1)")
        .bind(tenant_id)
        .fetch_one(&state.pool)
        .await?;
    if !exists {
        return Err(AppError::NotFound("Tenant not found".to_string()));
    }

    let mut tx = state.pool.begin().await?;

    sqlx::query("DELETE FROM provider_tenant_assignments WHERE tenant_id = $1")
        .bind(tenant_id)
        .execute(&mut *tx)
        .await?;

    for (i, provider_id) in req.provider_ids.iter().enumerate() {
        sqlx::query(
            r#"INSERT INTO provider_tenant_assignments (provider_id, tenant_id, is_default)
               VALUES ($1, $2, $3)
               ON CONFLICT (provider_id, tenant_id) DO NOTHING"#,
        )
        .bind(provider_id)
        .bind(tenant_id)
        .bind(i == 0)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    Ok(Json(serde_json::json!({
        "message": "Tenant provider assignments updated",
        "tenant_id": tenant_id,
        "assigned_count": req.provider_ids.len()
    })))
}

// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct SetDefaultProviderRequest {
    pub provider_id: Uuid,
}

/// PUT /api/tenants/:id/providers/default — set the default provider for a tenant
pub async fn set_tenant_default_provider(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(tenant_id): Path<Uuid>,
    Json(req): Json<SetDefaultProviderRequest>,
) -> AppResult<Json<serde_json::Value>> {
    if !auth_user.is_super_admin() && auth_user.tenant_id.as_ref() != Some(&tenant_id) {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }

    let mut tx = state.pool.begin().await?;

    // Clear current default for this tenant
    sqlx::query(
        "UPDATE provider_tenant_assignments SET is_default = false WHERE tenant_id = $1",
    )
    .bind(tenant_id)
    .execute(&mut *tx)
    .await?;

    // Set the new default — row must already exist (assignment must exist)
    let updated = sqlx::query(
        r#"UPDATE provider_tenant_assignments
           SET is_default = true
           WHERE tenant_id = $1 AND provider_id = $2"#,
    )
    .bind(tenant_id)
    .bind(req.provider_id)
    .execute(&mut *tx)
    .await?;

    if updated.rows_affected() == 0 {
        tx.rollback().await?;
        return Err(AppError::NotFound(
            "Provider is not assigned to this tenant".to_string(),
        ));
    }

    tx.commit().await?;

    Ok(Json(serde_json::json!({
        "message": "Default provider updated",
        "tenant_id": tenant_id,
        "provider_id": req.provider_id
    })))
}
