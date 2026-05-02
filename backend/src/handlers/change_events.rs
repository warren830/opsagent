//! HTTP handlers for the `change_events` global stream (W10).
//!
//! Read path only — writes happen from inside the event-source handlers
//! (argocd_webhook, rollout, alert_ingestion, catalog import). Keeping
//! write access out of a public endpoint means a misbehaving client can't
//! stuff synthetic "deploy" events into a tenant's audit log.

use axum::{
    Json,
    extract::{Query, State},
};

use crate::AppState;
use crate::error::AppResult;
use crate::middleware::auth::AuthUser;
use crate::models::change_event::{ChangeEvent, QueryChangesParams};
use crate::services::change_events;

/// GET /api/change-events
///
/// Lists change events, tenant-scoped. Super-admin sees everything; other
/// users see only rows inside their tenant. Filter by `service_id`, `kind`,
/// `since`, `until`, and `limit` (max 500, default 100).
pub async fn list(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Query(params): Query<QueryChangesParams>,
) -> AppResult<Json<Vec<ChangeEvent>>> {
    let tenant_filter = if auth_user.is_super_admin() {
        None
    } else {
        auth_user.tenant_id
    };

    let rows = change_events::query(&state.pool, params, tenant_filter).await?;
    Ok(Json(rows))
}
