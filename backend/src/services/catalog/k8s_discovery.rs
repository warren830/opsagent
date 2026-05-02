//! Scan a cluster for workloads and build `DiscoveredComponent` records.
//!
//! The goal is to auto-populate `catalog_entities` with placeholder
//! Component entries when a team hasn't written a `catalog-info.yaml`
//! yet — they can refine later. Grouping is by
//! `app.kubernetes.io/name` (Kubernetes recommended label) with a
//! fallback to the workload name when that label is missing.
//!
//! Argo Rollouts use a dynamic CRD lookup: if the CRD is absent the
//! scan logs a debug line and continues, it is not an error.
//!
//! Errors from individual workload kinds (Deployments / StatefulSets /
//! Rollouts) are collected into `DiscoveryResult.errors` so the caller
//! can still surface partial results — the discovery should never fail
//! wholesale just because one API call blew up.

use k8s_openapi::api::apps::v1::{Deployment, StatefulSet};
use kube::api::{Api, DynamicObject, ListParams};
use kube::discovery::ApiResource;
use std::collections::HashMap;
use uuid::Uuid;

use crate::error::{AppError, AppResult};

const APP_NAME_LABEL: &str = "app.kubernetes.io/name";
const PART_OF_LABEL: &str = "app.kubernetes.io/part-of";
const OWNER_LABEL: &str = "app.kubernetes.io/owner";

/// System namespaces we skip — they're cluster plumbing, not user
/// workloads that deserve a Catalog entry.
const EXCLUDED_NAMESPACES: &[&str] = &[
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

#[derive(Debug, Clone)]
pub struct DiscoveredComponent {
    pub name: String,
    pub cluster_id: Uuid,
    pub namespace: String,
    pub workload_name: String,
    pub system_hint: Option<String>,
    pub owner_hint: Option<String>,
}

pub struct DiscoveryResult {
    pub discovered: Vec<DiscoveredComponent>,
    pub errors: Vec<String>,
}

/// Argo Rollouts CRD descriptor — reused from `services::rollout` to
/// keep the two scans in sync if the CRD version ever bumps.
fn rollout_api_resource() -> ApiResource {
    ApiResource {
        group: "argoproj.io".to_string(),
        version: "v1alpha1".to_string(),
        api_version: "argoproj.io/v1alpha1".to_string(),
        kind: "Rollout".to_string(),
        plural: "rollouts".to_string(),
    }
}

/// Load cluster, build a kube client, scan Deployments + StatefulSets +
/// Rollouts, and return deduplicated `DiscoveredComponent` candidates.
pub async fn discover_cluster(
    pool: &sqlx::PgPool,
    cluster_id: Uuid,
) -> AppResult<DiscoveryResult> {
    let cluster = sqlx::query_as::<_, crate::models::cluster::Cluster>(
        "SELECT * FROM clusters WHERE id = $1",
    )
    .bind(cluster_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Cluster not found: {cluster_id}")))?;

    let client = crate::services::k8s::build_k8s_client(pool, &cluster).await?;

    // Keyed by (namespace, app-label or workload-name) so a Deployment
    // and a StatefulSet that share an `app.kubernetes.io/name` collapse
    // into a single Component candidate.
    let mut seen: HashMap<(String, String), DiscoveredComponent> = HashMap::new();
    let mut errors: Vec<String> = Vec::new();

    // ─── Deployments ─────────────────────────────────────────────
    match Api::<Deployment>::all(client.clone())
        .list(&ListParams::default())
        .await
    {
        Ok(deploys) => {
            for d in &deploys.items {
                let Some(ns) = d.metadata.namespace.as_deref() else { continue };
                if EXCLUDED_NAMESPACES.contains(&ns) {
                    continue;
                }
                let workload_name = d.metadata.name.as_deref().unwrap_or("unknown");
                let labels = d
                    .metadata
                    .labels
                    .as_ref()
                    .cloned()
                    .unwrap_or_default();
                let annotations = d
                    .metadata
                    .annotations
                    .as_ref()
                    .cloned()
                    .unwrap_or_default();
                insert_candidate(
                    &mut seen,
                    cluster_id,
                    ns,
                    workload_name,
                    &labels,
                    &annotations,
                );
            }
        }
        Err(e) => errors.push(format!("deployments: {e}")),
    }

    // ─── StatefulSets ────────────────────────────────────────────
    match Api::<StatefulSet>::all(client.clone())
        .list(&ListParams::default())
        .await
    {
        Ok(sets) => {
            for s in &sets.items {
                let Some(ns) = s.metadata.namespace.as_deref() else { continue };
                if EXCLUDED_NAMESPACES.contains(&ns) {
                    continue;
                }
                let workload_name = s.metadata.name.as_deref().unwrap_or("unknown");
                let labels = s
                    .metadata
                    .labels
                    .as_ref()
                    .cloned()
                    .unwrap_or_default();
                let annotations = s
                    .metadata
                    .annotations
                    .as_ref()
                    .cloned()
                    .unwrap_or_default();
                insert_candidate(
                    &mut seen,
                    cluster_id,
                    ns,
                    workload_name,
                    &labels,
                    &annotations,
                );
            }
        }
        Err(e) => errors.push(format!("statefulsets: {e}")),
    }

    // ─── Argo Rollouts (best-effort, CRD may be absent) ──────────
    let ar = rollout_api_resource();
    let rollout_api: Api<DynamicObject> = Api::all_with(client.clone(), &ar);
    match rollout_api.list(&ListParams::default()).await {
        Ok(rollouts) => {
            for r in &rollouts.items {
                let Some(ns) = r.metadata.namespace.as_deref() else { continue };
                if EXCLUDED_NAMESPACES.contains(&ns) {
                    continue;
                }
                let workload_name = r.metadata.name.as_deref().unwrap_or("unknown");
                let labels = r
                    .metadata
                    .labels
                    .as_ref()
                    .cloned()
                    .unwrap_or_default();
                let annotations = r
                    .metadata
                    .annotations
                    .as_ref()
                    .cloned()
                    .unwrap_or_default();
                insert_candidate(
                    &mut seen,
                    cluster_id,
                    ns,
                    workload_name,
                    &labels,
                    &annotations,
                );
            }
        }
        Err(e) => {
            // Argo Rollouts CRD not installed is the common case, log at
            // debug only so discovery of vanilla clusters stays quiet.
            tracing::debug!("rollouts scan skipped: {e}");
        }
    }

    Ok(DiscoveryResult {
        discovered: seen.into_values().collect(),
        errors,
    })
}

/// Merge a workload into the dedup map. The first workload for a given
/// (ns, app-name) pair wins — any later Deployment/StatefulSet/Rollout
/// sharing the same app label is treated as the same Component.
fn insert_candidate(
    seen: &mut HashMap<(String, String), DiscoveredComponent>,
    cluster_id: Uuid,
    namespace: &str,
    workload_name: &str,
    labels: &std::collections::BTreeMap<String, String>,
    annotations: &std::collections::BTreeMap<String, String>,
) {
    let app_name = labels
        .get(APP_NAME_LABEL)
        .map(|s| s.as_str())
        .unwrap_or(workload_name)
        .to_string();
    let key = (namespace.to_string(), app_name.clone());

    if seen.contains_key(&key) {
        return;
    }

    let system_hint = labels.get(PART_OF_LABEL).cloned();
    let owner_hint = labels
        .get(OWNER_LABEL)
        .cloned()
        .or_else(|| annotations.get(OWNER_LABEL).cloned());

    seen.insert(
        key,
        DiscoveredComponent {
            name: app_name,
            cluster_id,
            namespace: namespace.to_string(),
            workload_name: workload_name.to_string(),
            system_hint,
            owner_hint,
        },
    );
}
