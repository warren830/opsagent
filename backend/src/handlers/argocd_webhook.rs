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
            detail,
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

    Ok(Json(serde_json::json!({
        "status": "ok",
        "action": action,
        "app": payload.app_name,
        "cluster_matched": cluster_id.is_some(),
    })))
}
