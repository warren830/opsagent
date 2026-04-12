//! Rollout status watcher — periodically polls Argo Rollout CRDs across all clusters,
//! detects phase/step changes, and records them as deployment_events.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

use kube::api::{Api, DynamicObject};
use sqlx::PgPool;

use crate::services::rollout::{record_event, rollout_api_resource};
use crate::models::cluster::Cluster;
use crate::services::k8s::build_k8s_client;

/// Snapshot of a rollout's state for change detection.
#[derive(Debug, Clone, PartialEq)]
struct RolloutSnapshot {
    phase: String,
    current_step: i64,
    replicas: i64,
    ready_replicas: i64,
    image: String,
}

/// Composite key for the snapshot map.
fn snapshot_key(cluster_id: &Uuid, namespace: &str, name: &str) -> String {
    format!("{}/{}/{}", cluster_id, namespace, name)
}

/// Entry point: runs forever, polling all clusters every N seconds.
pub async fn run_rollout_watcher(pool: PgPool) {
    let interval_secs: u64 = std::env::var("ROLLOUT_WATCH_INTERVAL_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(60);

    tracing::info!("Rollout watcher started (interval={}s)", interval_secs);

    let snapshots: Arc<Mutex<HashMap<String, RolloutSnapshot>>> = Arc::new(Mutex::new(HashMap::new()));

    let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
    // Skip first tick — let the server warm up and clusters get discovered
    interval.tick().await;

    loop {
        interval.tick().await;
        if let Err(e) = poll_all_clusters(&pool, &snapshots).await {
            tracing::error!("Rollout watcher cycle failed: {}", e);
        }
    }
}

async fn poll_all_clusters(
    pool: &PgPool,
    snapshots: &Arc<Mutex<HashMap<String, RolloutSnapshot>>>,
) -> Result<(), String> {
    let clusters = sqlx::query_as::<_, Cluster>("SELECT * FROM clusters WHERE status = 'active' AND cloud = 'aws'")
        .fetch_all(pool)
        .await
        .map_err(|e| format!("DB error: {e}"))?;

    if clusters.is_empty() {
        return Ok(());
    }

    tracing::debug!("Rollout watcher: scanning {} cluster(s)", clusters.len());

    for cluster in &clusters {
        if let Err(e) = poll_cluster(pool, cluster, snapshots).await {
            tracing::warn!("Rollout watcher: cluster {} ({}): {}", cluster.name, cluster.id, e);
        }
    }

    Ok(())
}

async fn poll_cluster(
    pool: &PgPool,
    cluster: &Cluster,
    snapshots: &Arc<Mutex<HashMap<String, RolloutSnapshot>>>,
) -> Result<(), String> {
    let client = build_k8s_client(pool, cluster)
        .await
        .map_err(|e| format!("k8s client: {e}"))?;

    let ar = rollout_api_resource();
    let api: Api<DynamicObject> = Api::all_with(client, &ar);

    let list = api
        .list(&Default::default())
        .await
        .map_err(|e| format!("list rollouts: {e}"))?;

    let mut snaps = snapshots.lock().await;

    for obj in &list.items {
        let name = obj.metadata.name.as_deref().unwrap_or("unknown");
        let namespace = obj.metadata.namespace.as_deref().unwrap_or("default");
        let key = snapshot_key(&cluster.id, namespace, name);

        let status = obj.data.get("status");
        let spec = obj.data.get("spec");

        let phase = status
            .and_then(|s| s.get("phase"))
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown")
            .to_string();

        let current_step = status
            .and_then(|s| s.get("currentStepIndex"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        let replicas = status
            .and_then(|s| s.get("replicas"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        let ready_replicas = status
            .and_then(|s| s.get("readyReplicas"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        // Extract current container image
        let image = spec
            .and_then(|s| s.get("template"))
            .and_then(|t| t.get("spec"))
            .and_then(|s| s.get("containers"))
            .and_then(|c| c.as_array())
            .and_then(|arr| arr.first())
            .and_then(|c| c.get("image"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let new_snap = RolloutSnapshot {
            phase: phase.clone(),
            current_step,
            replicas,
            ready_replicas,
            image: image.clone(),
        };

        if let Some(old_snap) = snaps.get(&key)
            && *old_snap != new_snap
        {
            // Determine what changed
            let mut changes = Vec::new();
            if old_snap.phase != new_snap.phase {
                changes.push(format!("phase: {} → {}", old_snap.phase, new_snap.phase));
            }
            if old_snap.current_step != new_snap.current_step {
                changes.push(format!("step: {} → {}", old_snap.current_step, new_snap.current_step));
            }
            if old_snap.image != new_snap.image {
                changes.push(format!("image: {} → {}", old_snap.image, new_snap.image));
            }
            if old_snap.replicas != new_snap.replicas || old_snap.ready_replicas != new_snap.ready_replicas {
                changes.push(format!(
                    "replicas: {}/{} → {}/{}",
                    old_snap.ready_replicas, old_snap.replicas, new_snap.ready_replicas, new_snap.replicas
                ));
            }

            let action = if old_snap.phase != new_snap.phase {
                "phase_change"
            } else if old_snap.current_step != new_snap.current_step {
                "step_advance"
            } else if old_snap.image != new_snap.image {
                "image_update"
            } else {
                "replica_change"
            };

            let detail = serde_json::json!({
                "changes": changes,
                "phase": new_snap.phase,
                "step": new_snap.current_step,
                "replicas": new_snap.replicas,
                "ready_replicas": new_snap.ready_replicas,
                "image": new_snap.image,
            });

            tracing::info!(
                "Rollout change detected: {}/{} on {} — {}",
                namespace,
                name,
                cluster.name,
                changes.join(", ")
            );

            record_event(
                pool,
                cluster.id,
                namespace,
                name,
                action,
                detail,
                None,
                cluster.tenant_id,
            )
            .await;
        }
        // else: first time seeing this rollout — just record baseline, no event

        snaps.insert(key, new_snap);
    }

    Ok(())
}
