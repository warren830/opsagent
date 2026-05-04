//! HTTP handlers for the Services v2 overview endpoint.
//!
//! Two routes live here, both under `/api/services/overview`:
//!
//! - `GET /api/services/overview` — full tenant rollup.
//! - `GET /api/services/overview/{id}` — single Component rollup.
//!
//! Tenant isolation mirrors the rest of the stack: super_admin sees every
//! Component, everyone else is filtered by `auth_user.tenant_id`. Both
//! endpoints delegate to the aggregator in
//! `services::services_view::aggregator`.

use axum::{
    Json,
    extract::{Path, State},
};
use uuid::Uuid;

use crate::AppState;
use crate::error::{AppError, AppResult};
use crate::middleware::auth::AuthUser;
use crate::models::services_view::{ComponentOverview, ServicesOverviewResponse};
use crate::services::services_view::aggregator;

/// `GET /api/services/overview` — list every Component the caller can see,
/// grouped into Systems, with SLO budget / active incident / runtime probe
/// state pre-joined so the UI renders in a single round-trip (design §3.3).
pub async fn overview(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
) -> AppResult<Json<ServicesOverviewResponse>> {
    // `None` => super_admin view (no tenant filter); otherwise scope to
    // the user's tenant. Users with no tenant context still get `None`
    // here because super_admins sometimes don't have a tenant — same
    // pattern used throughout handlers/slo.rs.
    let tenant_filter = if auth_user.is_super_admin() {
        None
    } else {
        // Regular users MUST have a tenant context to see anything —
        // returning Forbidden here matches the convention used by
        // incident/slo handlers when a caller is missing tenant context.
        match auth_user.tenant_id {
            Some(tid) => Some(tid),
            None => {
                return Err(AppError::Forbidden("No tenant context".to_string()));
            }
        }
    };

    let resp = aggregator::build_overview(&state.pool, tenant_filter).await?;
    Ok(Json(resp))
}

/// `GET /api/services/overview/{id}` — single-Component variant. Useful
/// for the Component detail page header + tabs so the same aggregation
/// logic powers both views.
///
/// v1 implementation: call the full aggregator and filter to the one id.
/// Given the performance budget (~100 Components in 500ms) this is cheap
/// and keeps a single code path; a dedicated single-row query can be
/// added later if the endpoint becomes hot.
pub async fn overview_one(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<ComponentOverview>> {
    let tenant_filter = if auth_user.is_super_admin() {
        None
    } else {
        match auth_user.tenant_id {
            Some(tid) => Some(tid),
            None => {
                return Err(AppError::Forbidden("No tenant context".to_string()));
            }
        }
    };

    let resp = aggregator::build_overview(&state.pool, tenant_filter).await?;
    resp.components
        .into_iter()
        .find(|c| c.id == id)
        .map(Json)
        .ok_or_else(|| AppError::NotFound("Component not found".to_string()))
}
