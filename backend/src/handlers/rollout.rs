use axum::{
    Json,
    extract::{Path, State},
};
use uuid::Uuid;

use crate::AppState;
use crate::error::AppResult;
use crate::middleware::auth::AuthUser;
use crate::models::deployment_event::DeploymentEvent;
use crate::models::rollout::{
    AnalysisRunSummary, ChangeStrategyRequest, PromoteRequest, RolloutDetail, RolloutSummary,
};
use crate::services;

// ─── GET /api/clusters/{id}/rollouts ─────────────────────────────────────────

pub async fn list_rollouts(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(cluster_id): Path<Uuid>,
) -> AppResult<Json<Vec<RolloutSummary>>> {
    let rollouts = services::rollout::list_rollouts(&state.pool, &auth_user, cluster_id).await?;
    Ok(Json(rollouts))
}

// ─── GET /api/clusters/{id}/rollouts/{ns}/{name} ─────────────────────────────

pub async fn get_rollout(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path((cluster_id, ns, name)): Path<(Uuid, String, String)>,
) -> AppResult<Json<RolloutDetail>> {
    let detail = services::rollout::get_rollout(&state.pool, &auth_user, cluster_id, &ns, &name).await?;
    Ok(Json(detail))
}

// ─── GET /api/clusters/{id}/rollouts/{ns}/{name}/analysis ────────────────────

pub async fn list_analysis_runs(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path((cluster_id, ns, name)): Path<(Uuid, String, String)>,
) -> AppResult<Json<Vec<AnalysisRunSummary>>> {
    let runs = services::rollout::list_analysis_runs(&state.pool, &auth_user, cluster_id, &ns, &name).await?;
    Ok(Json(runs))
}

// ─── POST /api/clusters/{id}/rollouts/{ns}/{name}/promote ────────────────────

pub async fn promote(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path((cluster_id, ns, name)): Path<(Uuid, String, String)>,
    Json(req): Json<PromoteRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let result = services::rollout::promote(&state.pool, &auth_user, cluster_id, &ns, &name, req.full).await?;
    Ok(Json(result))
}

// ─── POST /api/clusters/{id}/rollouts/{ns}/{name}/rollback ───────────────────

pub async fn rollback(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path((cluster_id, ns, name)): Path<(Uuid, String, String)>,
) -> AppResult<Json<serde_json::Value>> {
    let result = services::rollout::rollback(&state.pool, &auth_user, cluster_id, &ns, &name).await?;
    Ok(Json(result))
}

// ─── POST /api/clusters/{id}/rollouts/{ns}/{name}/strategy ──────────────────

pub async fn change_strategy(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path((cluster_id, ns, name)): Path<(Uuid, String, String)>,
    Json(req): Json<ChangeStrategyRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let result = services::rollout::change_strategy(&state.pool, &auth_user, cluster_id, &ns, &name, req).await?;
    Ok(Json(result))
}

// ─── GET /api/deployment-events ──────────────────────────────────────────────

pub async fn list_events(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> AppResult<Json<Vec<DeploymentEvent>>> {
    let cluster_id = params.get("cluster_id").and_then(|v| Uuid::parse_str(v).ok());
    let ns = params.get("namespace").map(|s| s.as_str());
    let name = params.get("rollout_name").map(|s| s.as_str());
    let events = services::rollout::list_events(&state.pool, &auth_user, cluster_id, ns, name).await?;
    Ok(Json(events))
}
