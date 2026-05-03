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

    // W4: forward into any matching active incident timeline.
    let actor = crate::services::incident::timeline::user_actor(auth_user.user_id, &auth_user.username);
    let summary = format!(
        "Promote {} rollout {}/{}",
        if req.full { "full" } else { "step" },
        ns,
        name
    );
    crate::services::incident::timeline::fanout_deploy_event_to_incidents(
        &state.pool,
        &state.timeline_bus,
        auth_user.tenant_id,
        &name,
        crate::services::incident::timeline::KIND_PROMOTE_INITIATED,
        actor,
        &summary,
        serde_json::json!({
            "cluster_id": cluster_id,
            "namespace": ns,
            "rollout_name": name,
            "full": req.full,
        }),
    )
    .await;

    // W10: also record in the global change-events stream so the agent can
    // correlate this rollout action with non-incident SLO burns / alerts.
    let (service_id, service_tenant) = resolve_service_id(&state.pool, &name).await;
    let ce_actor = serde_json::json!({
        "type": "user",
        "id": auth_user.user_id,
        "display_name": auth_user.username,
    });
    crate::services::change_events::record_best_effort(
        &state.pool,
        service_tenant.or(auth_user.tenant_id),
        crate::models::change_event::KIND_DEPLOY,
        service_id,
        ce_actor,
        summary.clone(),
        serde_json::json!({
            "cluster_id": cluster_id,
            "namespace": ns,
            "rollout_name": name,
            "full": req.full,
        }),
        crate::models::change_event::SOURCE_ROLLOUT_API,
        None,
    )
    .await;

    Ok(Json(result))
}

// ─── POST /api/clusters/{id}/rollouts/{ns}/{name}/rollback ───────────────────

pub async fn rollback(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path((cluster_id, ns, name)): Path<(Uuid, String, String)>,
) -> AppResult<Json<serde_json::Value>> {
    let result = services::rollout::rollback(&state.pool, &auth_user, cluster_id, &ns, &name).await?;

    // W4: forward into any matching active incident timeline.
    let actor = crate::services::incident::timeline::user_actor(auth_user.user_id, &auth_user.username);
    let summary = format!("Rollback rollout {}/{}", ns, name);
    crate::services::incident::timeline::fanout_deploy_event_to_incidents(
        &state.pool,
        &state.timeline_bus,
        auth_user.tenant_id,
        &name,
        crate::services::incident::timeline::KIND_ROLLBACK_INITIATED,
        actor,
        &summary,
        serde_json::json!({
            "cluster_id": cluster_id,
            "namespace": ns,
            "rollout_name": name,
        }),
    )
    .await;

    // W10: also record on the global change-events stream.
    let (service_id, service_tenant) = resolve_service_id(&state.pool, &name).await;
    let ce_actor = serde_json::json!({
        "type": "user",
        "id": auth_user.user_id,
        "display_name": auth_user.username,
    });
    crate::services::change_events::record_best_effort(
        &state.pool,
        service_tenant.or(auth_user.tenant_id),
        crate::models::change_event::KIND_ROLLBACK,
        service_id,
        ce_actor,
        summary,
        serde_json::json!({
            "cluster_id": cluster_id,
            "namespace": ns,
            "rollout_name": name,
        }),
        crate::models::change_event::SOURCE_ROLLOUT_API,
        None,
    )
    .await;

    Ok(Json(result))
}

/// Best-effort lookup: map a rollout/workload name to the first matching
/// `catalog_entities` row. Returns `(service_id, tenant_id)` so the caller
/// can wire the `change_events.tenant_id` column; when the name is not in
/// the catalog we still record with `NULL` service_id.
async fn resolve_service_id(
    pool: &sqlx::PgPool,
    name: &str,
) -> (Option<Uuid>, Option<Uuid>) {
    match sqlx::query_as::<_, (Uuid, Uuid)>(
        "SELECT id, tenant_id FROM catalog_entities WHERE name = $1 LIMIT 1",
    )
    .bind(name)
    .fetch_optional(pool)
    .await
    {
        Ok(Some((id, t))) => (Some(id), Some(t)),
        _ => (None, None),
    }
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
