use axum::{
    Json,
    extract::{Path, State},
};
use uuid::Uuid;

use crate::AppState;
use crate::error::AppResult;
use crate::middleware::auth::AuthUser;
use crate::models::cluster::{Cluster, CreateClusterRequest, UpdateClusterRequest};
use crate::services;

/// GET /api/clusters
pub async fn list(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
) -> AppResult<Json<Vec<Cluster>>> {
    let rows = services::cluster::list(&state.pool, &auth_user).await?;
    Ok(Json(rows))
}

/// POST /api/clusters
pub async fn create(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Json(req): Json<CreateClusterRequest>,
) -> AppResult<Json<Cluster>> {
    let row = services::cluster::create(&state.pool, &auth_user, req).await?;
    Ok(Json(row))
}

/// PUT /api/clusters/:id
pub async fn update(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateClusterRequest>,
) -> AppResult<Json<Cluster>> {
    let row = services::cluster::update(&state.pool, &auth_user, id, req).await?;
    Ok(Json(row))
}

/// DELETE /api/clusters/:id
pub async fn delete(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    services::cluster::delete(&state.pool, &auth_user, id).await?;
    Ok(Json(serde_json::json!({"message": "Cluster deleted"})))
}

/// POST /api/clusters/discover
pub async fn discover(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
) -> AppResult<Json<services::cluster::DiscoverResult>> {
    let tenant_filter = if auth_user.is_super_admin() {
        None
    } else {
        auth_user.tenant_id
    };

    let result = services::cluster::discover_all_clusters(&state.pool, tenant_filter).await?;
    Ok(Json(result))
}
