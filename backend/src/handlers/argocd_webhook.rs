use axum::{Json, extract::State};
use serde::Deserialize;
use uuid::Uuid;

use crate::AppState;
use crate::error::AppResult;

// ─── ArgoCD Notification Webhook ─────────────────────────────────────────────
//
// ArgoCD sends notifications via webhook when sync/health status changes.
// This endpoint receives those events and records them as deployment_events.
// No authentication — ArgoCD cannot send JWT. Secured via network policy (cluster-internal).

/// Payload sent by ArgoCD notification webhook template.
/// The template is configured in argocd-values.yaml to send this shape.
#[derive(Debug, Deserialize)]
pub struct ArgocdWebhookPayload {
    /// ArgoCD Application name (e.g. "ops-backend")
    pub app_name: String,
    /// ArgoCD project (e.g. "default")
    #[serde(default)]
    pub project: Option<String>,
    /// Sync status: "Synced", "OutOfSync", "Unknown"
    #[serde(default)]
    pub sync_status: Option<String>,
    /// Health status: "Healthy", "Degraded", "Progressing", "Suspended", "Missing"
    #[serde(default)]
    pub health_status: Option<String>,
    /// Git revision (commit SHA)
    #[serde(default)]
    pub revision: Option<String>,
    /// Destination server URL (e.g. "https://XXXX.eks.amazonaws.com")
    #[serde(default)]
    pub dest_server: Option<String>,
    /// Destination namespace
    #[serde(default)]
    pub dest_namespace: Option<String>,
    /// Human-readable message from ArgoCD
    #[serde(default)]
    pub message: Option<String>,
}

/// POST /api/webhooks/argocd — receive ArgoCD notification events.
pub async fn receive(
    State(state): State<AppState>,
    Json(payload): Json<ArgocdWebhookPayload>,
) -> AppResult<Json<serde_json::Value>> {
    tracing::info!(
        "ArgoCD webhook: app={} sync={:?} health={:?} rev={:?}",
        payload.app_name,
        payload.sync_status,
        payload.health_status,
        payload.revision,
    );

    // Determine action from sync/health status
    let action = match (payload.sync_status.as_deref(), payload.health_status.as_deref()) {
        (Some("Synced"), Some("Healthy")) => "argocd_sync_success",
        (Some("Synced"), Some("Degraded")) => "argocd_sync_degraded",
        (Some("Synced"), _) => "argocd_sync_success",
        (Some("OutOfSync"), _) => "argocd_out_of_sync",
        (_, Some("Degraded")) => "argocd_health_degraded",
        (_, Some("Progressing")) => "argocd_progressing",
        _ => "argocd_event",
    };

    // Try to match cluster by destination server URL → clusters.config->>'endpoint'
    let cluster_id: Option<Uuid> = if let Some(ref server) = payload.dest_server {
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM clusters WHERE config->>'endpoint' = $1 LIMIT 1")
            .bind(server)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None)
    } else {
        None
    };

    let namespace = payload.dest_namespace.as_deref().unwrap_or("default");

    let detail = serde_json::json!({
        "app_name": payload.app_name,
        "project": payload.project,
        "sync_status": payload.sync_status,
        "health_status": payload.health_status,
        "revision": payload.revision,
        "dest_server": payload.dest_server,
        "message": payload.message,
    });

    // Record event — cluster_id may be null if we couldn't match the server URL
    if let Some(cid) = cluster_id {
        crate::services::rollout::record_event(
            &state.pool,
            cid,
            namespace,
            &payload.app_name,
            action,
            detail.clone(),
            None, // no user — automated event
            None, // tenant determined by cluster
        )
        .await;
    } else {
        // Still record with a nil cluster_id — better to have the event than lose it
        // Use a nil UUID as placeholder
        tracing::warn!(
            "ArgoCD webhook: could not match dest_server {:?} to a cluster",
            payload.dest_server
        );
        if let Err(e) = sqlx::query(
            r#"INSERT INTO deployment_events (cluster_id, namespace, rollout_name, action, detail)
               VALUES ($1, $2, $3, $4, $5)"#,
        )
        .bind(Uuid::nil())
        .bind(namespace)
        .bind(&payload.app_name)
        .bind(action)
        .bind(&detail)
        .execute(&state.pool)
        .await
        {
            tracing::error!("Failed to record ArgoCD event: {}", e);
        }
    }

    // W4: fan this event out to any active incident whose affected
    // components include the ArgoCD app. Non-blocking — logs on failure.
    let timeline_kind = match action {
        "argocd_sync_success" => crate::services::incident::timeline::KIND_DEPLOY_SUCCEEDED,
        "argocd_sync_degraded"
        | "argocd_health_degraded"
        | "argocd_out_of_sync" => crate::services::incident::timeline::KIND_DEPLOY_FAILED,
        "argocd_progressing" => crate::services::incident::timeline::KIND_DEPLOY_STARTED,
        _ => crate::services::incident::timeline::KIND_DEPLOYMENT,
    };
    let actor = crate::services::incident::timeline::system_actor("argocd_webhook");
    let summary = format!(
        "ArgoCD {} for {} (rev {})",
        action,
        payload.app_name,
        payload.revision.as_deref().unwrap_or("?")
    );
    crate::services::incident::timeline::fanout_deploy_event_to_incidents(
        &state.pool,
        &state.timeline_bus,
        &payload.app_name,
        timeline_kind,
        actor,
        &summary,
        detail.clone(),
    )
    .await;

    // W10: also append to the global change_events stream, independent of
    // whether an incident was open. The per-incident fan-out above covers
    // "what happened during the war room"; this row powers "what recently
    // changed for service X?" regardless of incident status.
    let (service_id, service_tenant) = resolve_service_for_app(&state.pool, &payload.app_name).await;
    let ce_actor = serde_json::json!({
        "type": "system",
        "display_name": "argocd",
        "source": "argocd_webhook",
    });
    let ce_kind = match action {
        "argocd_sync_success"
        | "argocd_sync_degraded"
        | "argocd_progressing"
        | "argocd_out_of_sync"
        | "argocd_health_degraded" => crate::models::change_event::KIND_DEPLOY,
        _ => crate::models::change_event::KIND_DEPLOY,
    };
    crate::services::change_events::record_best_effort(
        &state.pool,
        service_tenant,
        ce_kind,
        service_id,
        ce_actor,
        format!(
            "Deploy {} to {} ({})",
            payload.app_name, namespace, action
        ),
        detail.clone(),
        crate::models::change_event::SOURCE_ARGOCD,
        payload.revision.clone(),
    )
    .await;

    Ok(Json(serde_json::json!({
        "status": "ok",
        "action": action,
        "app": payload.app_name,
        "cluster_matched": cluster_id.is_some(),
    })))
}

/// Best-effort resolution of an ArgoCD app / rollout / workload name to a
/// `catalog_entities` row. Returns `(service_id, tenant_id)`. Either side
/// may be `None` — a missing catalog row is fine, the `change_events` row
/// is still written without the service linkage.
async fn resolve_service_for_app(
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
