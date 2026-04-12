use kube::api::{Api, DynamicObject, Patch, PatchParams};
use kube::discovery::ApiResource;

use crate::services::k8s::build_k8s_client;

fn rollout_api_resource() -> ApiResource {
    ApiResource {
        group: "argoproj.io".to_string(),
        version: "v1alpha1".to_string(),
        api_version: "argoproj.io/v1alpha1".to_string(),
        kind: "Rollout".to_string(),
        plural: "rollouts".to_string(),
    }
}

/// When a critical/high alert fires, check if any Rollouts in the affected namespace
/// are currently Progressing and pause them automatically.
///
/// Returns a list of paused rollout names (e.g., ["default/my-app"]).
pub async fn check_and_pause_rollouts(pool: &sqlx::PgPool, namespace: &str) -> Vec<String> {
    let mut paused = Vec::new();

    // Get all clusters to check
    let clusters = match sqlx::query_as::<_, crate::models::cluster::Cluster>(
        "SELECT * FROM clusters WHERE cloud = 'aws' AND cluster_type = 'eks'",
    )
    .fetch_all(pool)
    .await
    {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("rollout_guard: failed to fetch clusters: {e}");
            return paused;
        }
    };

    for cluster in &clusters {
        let client = match build_k8s_client(pool, cluster).await {
            Ok(c) => c,
            Err(e) => {
                tracing::debug!(
                    "rollout_guard: skipping cluster {} (cannot build client: {e})",
                    cluster.name
                );
                continue;
            }
        };

        let ar = rollout_api_resource();
        let api: Api<DynamicObject> = Api::namespaced_with(client, namespace, &ar);

        let list = match api.list(&Default::default()).await {
            Ok(l) => l,
            Err(e) => {
                tracing::debug!(
                    "rollout_guard: cannot list rollouts in {}/{}: {e}",
                    cluster.name,
                    namespace
                );
                continue;
            }
        };

        for obj in &list.items {
            let raw = match serde_json::to_value(obj) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let phase = raw.pointer("/status/phase").and_then(|v| v.as_str()).unwrap_or("");

            if phase != "Progressing" {
                continue;
            }

            let name = obj.metadata.name.as_deref().unwrap_or("unknown");

            // Pause the rollout
            let patch = serde_json::json!({
                "status": {
                    "controllerPause": true,
                    "pauseConditions": [{
                        "reason": "OpsAlertGuard",
                        "startTime": chrono::Utc::now().to_rfc3339(),
                    }]
                }
            });

            let pp = PatchParams::default();
            match api.patch_status(name, &pp, &Patch::Merge(&patch)).await {
                Ok(_) => {
                    tracing::warn!(
                        "rollout_guard: PAUSED rollout {}/{} in cluster {} due to critical alert",
                        namespace,
                        name,
                        cluster.name
                    );
                    paused.push(format!("{}/{}", namespace, name));
                }
                Err(e) => {
                    tracing::warn!("rollout_guard: failed to pause {}/{}: {e}", namespace, name);
                }
            }
        }
    }

    paused
}

/// Extract namespace from alert metadata.
/// Supports Grafana labels.namespace, Datadog tags kube_namespace, Dynatrace impacted_entities.
pub fn extract_namespace_from_alert(meta: &serde_json::Value) -> Option<String> {
    // Grafana: labels.namespace
    if let Some(ns) = meta.pointer("/labels/namespace").and_then(|v| v.as_str()) {
        return Some(ns.to_string());
    }

    // Datadog: tags contain "kube_namespace:xxx"
    if let Some(tags) = meta.get("tags").and_then(|v| v.as_str()) {
        for tag in tags.split(',') {
            let tag = tag.trim();
            if let Some(ns) = tag.strip_prefix("kube_namespace:") {
                return Some(ns.to_string());
            }
        }
    }

    // Dynatrace: impacted_entities may contain namespace info
    if let Some(entities) = meta.get("impacted_entities").and_then(|v| v.as_array()) {
        for entity in entities {
            if let Some(name) = entity.get("name").and_then(|v| v.as_str()) {
                // Dynatrace entity names often include namespace
                if name.contains('/') {
                    return name.split('/').next().map(|s| s.to_string());
                }
            }
        }
    }

    None
}
