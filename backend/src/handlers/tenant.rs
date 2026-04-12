use axum::{
    Json,
    extract::{Path, State},
};
use uuid::Uuid;

use crate::AppState;
use crate::error::AppResult;
use crate::middleware::auth::AuthUser;
use crate::models::tenant::{CreateTenantRequest, Tenant, UpdateTenantRequest};
use crate::services;

/// GET /api/tenants
pub async fn list_tenants(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
) -> AppResult<Json<Vec<Tenant>>> {
    let tenants = services::tenant::list(&state.pool, &auth_user).await?;
    Ok(Json(tenants))
}

/// POST /api/tenants (super_admin only)
pub async fn create_tenant(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Json(req): Json<CreateTenantRequest>,
) -> AppResult<Json<Tenant>> {
    let tenant = services::tenant::create(&state.pool, &auth_user, req).await?;
    Ok(Json(tenant))
}

/// GET /api/tenants/:id
pub async fn get_tenant(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Tenant>> {
    let tenant = services::tenant::get(&state.pool, &auth_user, id).await?;
    Ok(Json(tenant))
}

/// PUT /api/tenants/:id
pub async fn update_tenant(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateTenantRequest>,
) -> AppResult<Json<Tenant>> {
    let tenant = services::tenant::update(&state.pool, &auth_user, id, req).await?;
    Ok(Json(tenant))
}

/// DELETE /api/tenants/:id (super_admin only)
pub async fn delete_tenant(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    services::tenant::delete(&state.pool, &auth_user, id).await?;
    Ok(Json(serde_json::json!({"message": "Tenant deleted"})))
}
