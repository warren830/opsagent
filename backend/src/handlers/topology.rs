use axum::{Json, extract::State};
use k8s_openapi::api::apps::v1::Deployment;
use k8s_openapi::api::core::v1::Service;
use k8s_openapi::api::networking::v1::Ingress;
use kube::api::{Api, ListParams};
use serde::Serialize;
use std::sync::OnceLock;
use tokio::sync::RwLock;

use crate::AppState;
use crate::error::{AppError, AppResult};
use crate::middleware::auth::AuthUser;
use crate::models::cluster::Cluster;
use crate::services::k8s::build_k8s_client;

// ─── In-memory topology cache (persistent until manual refresh) ─────────────

static TOPOLOGY_CACHE: OnceLock<RwLock<Option<TopologyResponse>>> = OnceLock::new();

// ─── Response types ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct TopologyResponse {
    pub nodes: Vec<TopoNode>,
    pub edges: Vec<TopoEdge>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TopoNode {
    pub id: String,
    pub label: String,
    pub subtitle: Option<String>,
    pub kind: String, // "ingress" | "service" | "deployment" | "rollout"
    pub namespace: String,
    pub cluster: String,
    pub cluster_id: String,
    pub status: String, // "healthy" | "warning" | "critical" | "unknown"
    pub replicas: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TopoEdge {
    pub id: String,
    pub source: String,
    pub target: String,
    pub label: Option<String>,
}

// ─── GET /api/topology ──────────────────────────────────────────────────────

/// Build a service topology graph from all accessible clusters.
/// Discovers: Ingress → Service → Deployment/Rollout relationships.
/// Results are cached in memory until the user explicitly refreshes (?refresh=true).
pub async fn get_topology(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> AppResult<Json<TopologyResponse>> {
    let cache = TOPOLOGY_CACHE.get_or_init(|| RwLock::new(None));
    let force_refresh = params.get("refresh").is_some_and(|v| v == "true");

    // Return cache if available and not forcing refresh
    if !force_refresh {
        let guard = cache.read().await;
        if let Some(ref data) = *guard {
            return Ok(Json(data.clone()));
        }
    }

    // Fetch from K8s
    let result = fetch_topology(&auth_user, &state).await?;

    // Update cache
    {
        let mut guard = cache.write().await;
        *guard = Some(result.clone());
    }

    Ok(Json(result))
}

async fn fetch_topology(
    auth_user: &AuthUser,
    state: &AppState,
) -> AppResult<TopologyResponse> {
    // Get all clusters the user can access
    let clusters: Vec<Cluster> = if auth_user.is_super_admin() {
        sqlx::query_as("SELECT * FROM clusters WHERE UPPER(status) = 'ACTIVE'")
            .fetch_all(&state.pool)
            .await?
    } else {
        sqlx::query_as("SELECT * FROM clusters WHERE UPPER(status) = 'ACTIVE' AND tenant_id = $1")
            .bind(auth_user.tenant_id)
            .fetch_all(&state.pool)
            .await?
    };

    let mut nodes: Vec<TopoNode> = Vec::new();
    let mut edges: Vec<TopoEdge> = Vec::new();

    // Process each cluster in parallel
    let handles: Vec<_> = clusters
        .into_iter()
        .map(|cluster| {
            let pool = state.pool.clone();
            tokio::spawn(async move {
                match build_topology_for_cluster(&pool, &cluster).await {
                    Ok((n, e)) => (n, e),
                    Err(e) => {
                        tracing::warn!("Topology fetch failed for {}: {e}", cluster.name);
                        (vec![], vec![])
                    }
                }
            })
        })
        .collect();

    for handle in handles {
        if let Ok((n, e)) = handle.await {
            nodes.extend(n);
            edges.extend(e);
        }
    }

    Ok(TopologyResponse { nodes, edges })
}

/// Build topology nodes + edges for a single cluster.
async fn build_topology_for_cluster(
    pool: &sqlx::PgPool,
    cluster: &Cluster,
) -> AppResult<(Vec<TopoNode>, Vec<TopoEdge>)> {
    let client = build_k8s_client(pool, cluster).await?;
    let cluster_name = &cluster.name;
    let cluster_id = cluster.id.to_string();

    let mut nodes = Vec::new();
    let mut edges = Vec::new();

    // Exclude system namespaces
    let exclude_ns = [
        "kube-system",
        "kube-public",
        "kube-node-lease",
        "argo-rollouts",
        "argocd",
        "cert-manager",
        "external-secrets",
        "monitoring",
        "grafana",
        "loki",
        "mimir",
        "tempo",
        "ingress-nginx",
        "karpenter",
    ];

    // ─── Fetch Deployments ──────────────────────────────────────
    let deploy_api: Api<Deployment> = Api::all(client.clone());
    let deploys = deploy_api
        .list(&ListParams::default())
        .await
        .map_err(|e| AppError::Kubernetes(format!("List deployments: {e}")))?;

    for d in &deploys.items {
        let ns = d.metadata.namespace.as_deref().unwrap_or("default");
        if exclude_ns.contains(&ns) {
            continue;
        }

        let name = d.metadata.name.as_deref().unwrap_or("unknown");
        let spec = d.spec.as_ref();
        let status = d.status.as_ref();

        let desired = spec.and_then(|s| s.replicas).unwrap_or(1);
        let ready = status.and_then(|s| s.ready_replicas).unwrap_or(0);
        let available = status.and_then(|s| s.available_replicas).unwrap_or(0);

        let health = if available >= desired {
            "healthy"
        } else if ready > 0 {
            "warning"
        } else {
            "critical"
        };

        nodes.push(TopoNode {
            id: format!("{cluster_id}/deploy/{ns}/{name}"),
            label: name.to_string(),
            subtitle: Some(format!("{ready}/{desired} ready")),
            kind: "deployment".to_string(),
            namespace: ns.to_string(),
            cluster: cluster_name.clone(),
            cluster_id: cluster_id.clone(),
            status: health.to_string(),
            replicas: Some(format!("{ready}/{desired}")),
        });
    }

    // ─── Fetch Argo Rollouts (if CRD exists) ────────────────────
    {
        let ar = kube::discovery::ApiResource {
            group: "argoproj.io".to_string(),
            version: "v1alpha1".to_string(),
            api_version: "argoproj.io/v1alpha1".to_string(),
            kind: "Rollout".to_string(),
            plural: "rollouts".to_string(),
        };
        let rollout_api: Api<kube::api::DynamicObject> = Api::all_with(client.clone(), &ar);

        if let Ok(rollouts) = rollout_api.list(&ListParams::default()).await {
            for obj in &rollouts.items {
                let ns = obj.metadata.namespace.as_deref().unwrap_or("default");
                if exclude_ns.contains(&ns) {
                    continue;
                }

                let name = obj.metadata.name.as_deref().unwrap_or("unknown");
                let raw = serde_json::to_value(obj).unwrap_or_default();
                let phase = raw
                    .pointer("/status/phase")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown");

                let desired = raw.pointer("/spec/replicas").and_then(|v| v.as_i64()).unwrap_or(1);
                let ready = raw
                    .pointer("/status/readyReplicas")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);

                let health = match phase {
                    "Healthy" => "healthy",
                    "Progressing" => "warning",
                    "Paused" => "warning",
                    "Degraded" => "critical",
                    _ => "unknown",
                };

                let strategy = if raw.pointer("/spec/strategy/canary").is_some() {
                    "canary"
                } else if raw.pointer("/spec/strategy/blueGreen").is_some() {
                    "blueGreen"
                } else {
                    "unknown"
                };

                nodes.push(TopoNode {
                    id: format!("{cluster_id}/rollout/{ns}/{name}"),
                    label: name.to_string(),
                    subtitle: Some(format!("{phase} · {strategy}")),
                    kind: "rollout".to_string(),
                    namespace: ns.to_string(),
                    cluster: cluster_name.clone(),
                    cluster_id: cluster_id.clone(),
                    status: health.to_string(),
                    replicas: Some(format!("{ready}/{desired}")),
                });
            }
        }
    }

    // ─── Fetch Services ─────────────────────────────────────────
    let svc_api: Api<Service> = Api::all(client.clone());
    let services = svc_api
        .list(&ListParams::default())
        .await
        .map_err(|e| AppError::Kubernetes(format!("List services: {e}")))?;

    for s in &services.items {
        let ns = s.metadata.namespace.as_deref().unwrap_or("default");
        if exclude_ns.contains(&ns) {
            continue;
        }

        let name = s.metadata.name.as_deref().unwrap_or("unknown");
        let spec = s.spec.as_ref();
        let svc_type = spec.and_then(|s| s.type_.as_deref()).unwrap_or("ClusterIP");
        let ports: Vec<String> = spec
            .and_then(|s| s.ports.as_ref())
            .map(|ps| ps.iter().map(|p| format!("{}", p.port)).collect())
            .unwrap_or_default();

        let svc_id = format!("{cluster_id}/svc/{ns}/{name}");

        nodes.push(TopoNode {
            id: svc_id.clone(),
            label: name.to_string(),
            subtitle: Some(format!("{svc_type} · :{}", ports.join(","))),
            kind: "service".to_string(),
            namespace: ns.to_string(),
            cluster: cluster_name.clone(),
            cluster_id: cluster_id.clone(),
            status: "healthy".to_string(), // Services don't have direct health
            replicas: None,
        });

        // ── Link Service → Deployment/Rollout by matching selector to labels ──
        if let Some(selector) = spec.and_then(|s| s.selector.as_ref()) {
            // Try to match deployments
            for d in &deploys.items {
                let d_ns = d.metadata.namespace.as_deref().unwrap_or("default");
                if d_ns != ns {
                    continue;
                }

                let d_name = d.metadata.name.as_deref().unwrap_or("unknown");
                let tmpl_labels = d
                    .spec
                    .as_ref()
                    .and_then(|s| s.template.metadata.as_ref())
                    .and_then(|m| m.labels.as_ref());

                if let Some(labels) = tmpl_labels {
                    let matches = selector
                        .iter()
                        .all(|(k, v)| labels.get(k).map(|lv| lv == v).unwrap_or(false));
                    if matches {
                        edges.push(TopoEdge {
                            id: format!("e-{svc_id}-deploy-{d_name}"),
                            source: svc_id.clone(),
                            target: format!("{cluster_id}/deploy/{ns}/{d_name}"),
                            label: None,
                        });
                    }
                }
            }

            // Try to match rollouts (by name convention: svc name ≈ rollout name)
            let rollout_node_id = format!("{cluster_id}/rollout/{ns}/{name}");
            if nodes.iter().any(|n| n.id == rollout_node_id) {
                edges.push(TopoEdge {
                    id: format!("e-{svc_id}-rollout-{name}"),
                    source: svc_id.clone(),
                    target: rollout_node_id,
                    label: None,
                });
            }
        }
    }

    // ─── Fetch Ingresses ────────────────────────────────────────
    let ingress_api: Api<Ingress> = Api::all(client.clone());
    let ingresses = ingress_api
        .list(&ListParams::default())
        .await
        .map_err(|e| AppError::Kubernetes(format!("List ingresses: {e}")))?;

    for ing in &ingresses.items {
        let ns = ing.metadata.namespace.as_deref().unwrap_or("default");
        if exclude_ns.contains(&ns) {
            continue;
        }

        let name = ing.metadata.name.as_deref().unwrap_or("unknown");
        let spec = ing.spec.as_ref();

        // Extract hosts
        let hosts: Vec<String> = spec
            .and_then(|s| s.rules.as_ref())
            .map(|rules| rules.iter().filter_map(|r| r.host.clone()).collect())
            .unwrap_or_default();

        let ing_id = format!("{cluster_id}/ingress/{ns}/{name}");

        nodes.push(TopoNode {
            id: ing_id.clone(),
            label: name.to_string(),
            subtitle: if hosts.is_empty() { None } else { Some(hosts.join(", ")) },
            kind: "ingress".to_string(),
            namespace: ns.to_string(),
            cluster: cluster_name.clone(),
            cluster_id: cluster_id.clone(),
            status: "healthy".to_string(),
            replicas: None,
        });

        // Link Ingress → Service (from ingress rules)
        if let Some(rules) = spec.and_then(|s| s.rules.as_ref()) {
            for rule in rules {
                if let Some(http) = &rule.http {
                    for path in &http.paths {
                        if let Some(backend_svc) = path.backend.service.as_ref() {
                            let target_svc = &backend_svc.name;
                            let target_id = format!("{cluster_id}/svc/{ns}/{target_svc}");
                            let path_str = path.path.as_deref().unwrap_or("/*");

                            edges.push(TopoEdge {
                                id: format!("e-{ing_id}-{target_svc}"),
                                source: ing_id.clone(),
                                target: target_id,
                                label: Some(path_str.to_string()),
                            });
                        }
                    }
                }
            }
        }
    }

    Ok((nodes, edges))
}
