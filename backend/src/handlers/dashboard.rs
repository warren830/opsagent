use axum::{Json, extract::State};

use crate::AppState;
use crate::error::AppResult;
use crate::middleware::auth::AuthUser;
use crate::models::dashboard::DashboardStats;
use crate::services;

/// GET /api/dashboard/stats
/// Returns aggregated counts for the dashboard overview
pub async fn stats(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
) -> AppResult<Json<DashboardStats>> {
    let stats = services::dashboard::stats(&state.pool, &auth_user).await?;
    Ok(Json(stats))
}
