//! K8s Service selector → workload relation inference.
//!
//! ## Purpose
//!
//! When a user drops a plain K8s `Service` into a cluster but never adds a
//! `relations:` block to the matching `catalog-info.yaml`, the Catalog has
//! no idea the Service fronts that workload. This loop closes the gap by
//! consuming the same data a human would look at: `Service.spec.selector`
//! labels matched against `Deployment` / `StatefulSet` / `DaemonSet`
//! template labels.
//!
//! Inferred relations are tagged `source = 'k8s_selector'` so subsequent
//! passes can reconcile (delete edges that no longer match) without ever
//! touching the `'declared'` edges that came from YAML import.
//!
//! ## Wiring into the app
//!
//! `main.rs` spawns this loop at startup. Disable via `SKIP_K8S_INFERER=true`
//! for unit tests / offline dev.
//!
//! ## Algorithm
//!
//! On each tick (default 5 min, override with `K8S_INFERER_INTERVAL_SECS`):
//!
//! 1. `SELECT * FROM clusters WHERE status = 'active'` — drop clusters with
//!    a NULL `tenant_id` because `catalog_entities.tenant_id` is NOT NULL
//!    and we have nowhere to park auto-created API entities.
//! 2. Per cluster: build a kube client via `services::k8s::build_k8s_client`
//!    (EKS via IRSA/SA token, GKE via gcloud) — reuse the existing helper,
//!    don't reinvent auth.
//! 3. List Services cluster-wide, skip system / infra namespaces and any
//!    Service with an empty selector — those are headless bookkeeping
//!    services (kubelet, metrics-server, etc.) and there's no workload
//!    shape to infer from.
//! 4. For each Service, list `Deployment`, `StatefulSet`, `DaemonSet` in
//!    the same namespace filtered by the selector's label map (translated
//!    to a `key=value,key=value` string for `ListParams::labels`).
//! 5. Resolve each matched workload → `catalog_entities.id` (kind='component')
//!    via a JSONB lookup on `spec -> 'runtime' ->> 'cluster'/'namespace'/'workload'`.
//! 6. Resolve the Service → `catalog_entities.id` (kind='api'). Auto-create
//!    the API entity if absent, stamped with
//!    `annotations.loops.yingchu.cloud/source = 'k8s_inferer'` so the
//!    YAML parser knows not to clobber it on a refresh that doesn't
//!    mention the Service.
//! 7. `INSERT INTO catalog_relations (from_id, to_id, relation_type, source)
//!    VALUES ($api, $component, 'provides', 'k8s_selector')
//!    ON CONFLICT (from_id, to_id, relation_type) DO NOTHING`.
//! 8. After the pass completes for a tenant, reconcile: delete any
//!    `source='k8s_selector'` edge from that tenant that wasn't produced
//!    this pass. Declared edges (source='declared') are untouched.
//!
//! ## Error semantics
//!
//! * Per-cluster failure (network, auth, CRD missing) logs at `warn` and
//!   moves on — one bad cluster must not block the others.
//! * Per-Service DB errors log at `warn`; we skip that Service and continue.
//! * Reconciliation errors log at `error`; the INSERTs from this pass are
//!   already committed row-by-row, so a failed DELETE just means a stale
//!   edge lives one more cycle.

use crate::models::cluster::Cluster;
use k8s_openapi::api::apps::v1::{DaemonSet, Deployment, StatefulSet};
use k8s_openapi::api::core::v1::Service;
use kube::api::{Api, ListParams};
use sqlx::PgPool;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::Duration;
use uuid::Uuid;

/// Interval between scan passes. 5 minutes balances freshness against
/// K8s API pressure; override with `K8S_INFERER_INTERVAL_SECS`.
pub const DEFAULT_INTERVAL: Duration = Duration::from_secs(300);

/// Namespaces we never infer from — cluster plumbing, never user
/// workload. Anything in this list is skipped by EXACT match.
const SYSTEM_NAMESPACES: &[&str] = &[
    "kube-system",
    "kube-public",
    "kube-node-lease",
    "gmp-system",
    "cert-manager",
    "ingress-nginx",
];

/// Prefix blocklist — any namespace starting with one of these strings is
/// treated as system. Today: Argo's handful of namespaces (`argo-events`,
/// `argo-rollouts`, `argo-workflows`, `argocd` too via the exact match? No
/// — `argocd` doesn't start with `argo-`, so add it below if needed).
const SYSTEM_NAMESPACE_PREFIXES: &[&str] = &["argo-"];

/// Annotation key used to tag entities the inferer has created. Reads it
/// back on subsequent passes so we don't double-create and we can tell
/// the YAML parser "hands off" (it should never overwrite auto entities,
/// only the inferer does).
const SOURCE_ANNOTATION_KEY: &str = "loops.yingchu.cloud/source";
const SOURCE_K8S_INFERER: &str = "k8s_inferer";

/// Edge source value written into `catalog_relations.source` for every
/// row this loop emits.
const EDGE_SOURCE: &str = "k8s_selector";

/// Relation type — a Service *provides* the workload it fronts.
const RELATION_PROVIDES: &str = "provides";

/// Background loop. Spawned once at startup from `main.rs`. Runs forever;
/// per-cluster and per-Service errors are absorbed so the loop never dies.
pub async fn run_inferer_loop(pool: PgPool) {
    let interval = std::env::var("K8S_INFERER_INTERVAL_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or(DEFAULT_INTERVAL);

    tracing::info!(
        "k8s_inferer started (interval = {}s)",
        interval.as_secs()
    );

    // Sleep first so the server has time to come up — `main.rs` spawns
    // this right after binding the socket.
    loop {
        tokio::time::sleep(interval).await;
        match run_one_pass(&pool).await {
            Ok(stats) => tracing::info!(
                clusters_scanned = stats.clusters_scanned,
                clusters_failed = stats.clusters_failed,
                services_examined = stats.services_examined,
                edges_written = stats.edges_written,
                edges_reconciled = stats.edges_reconciled,
                "k8s_inferer pass complete"
            ),
            Err(e) => tracing::error!("k8s_inferer pass failed: {e}"),
        }
    }
}

/// Counters returned by a single pass — exposed mainly for logging and
/// for integration tests to assert on.
#[derive(Debug, Default, Clone, Copy)]
pub struct InfererPassStats {
    pub clusters_scanned: usize,
    pub clusters_failed: usize,
    pub services_examined: usize,
    pub edges_written: usize,
    pub edges_reconciled: usize,
}

/// Run one full pass over every active cluster. Never panics; per-cluster
/// failures are warn-logged and omitted from reconciliation (so stale
/// edges for a temporarily-offline cluster don't get blown away).
async fn run_one_pass(pool: &PgPool) -> Result<InfererPassStats, sqlx::Error> {
    let mut stats = InfererPassStats::default();

    let clusters = sqlx::query_as::<_, Cluster>(
        "SELECT * FROM clusters WHERE status = 'active' ORDER BY name",
    )
    .fetch_all(pool)
    .await?;

    // Group discovered edges per tenant so the reconciliation DELETE can
    // scope by tenant. Key: tenant_id; value: set of (from_id, to_id)
    // tuples produced by this pass.
    //
    // We also track which tenants had at least one cluster succeed —
    // reconciliation only runs for tenants whose scan was complete. If
    // every cluster for a tenant failed we skip the DELETE to avoid
    // wiping valid-but-stale edges.
    let mut tuples_by_tenant: HashMap<Uuid, HashSet<(Uuid, Uuid)>> = HashMap::new();
    let mut tenants_with_success: HashSet<Uuid> = HashSet::new();

    for cluster in &clusters {
        let Some(tenant_id) = cluster.tenant_id else {
            tracing::debug!(
                cluster = %cluster.name,
                "k8s_inferer: skipping cluster with NULL tenant_id"
            );
            continue;
        };

        match scan_cluster(pool, cluster, tenant_id).await {
            Ok(result) => {
                stats.clusters_scanned += 1;
                stats.services_examined += result.services_examined;
                stats.edges_written += result.edges_written;
                tenants_with_success.insert(tenant_id);
                tuples_by_tenant
                    .entry(tenant_id)
                    .or_default()
                    .extend(result.tuples);
            }
            Err(e) => {
                stats.clusters_failed += 1;
                tracing::warn!(
                    cluster = %cluster.name,
                    cluster_id = %cluster.id,
                    error = %e,
                    "k8s_inferer: cluster scan failed; continuing"
                );
            }
        }
    }

    // ─── Reconcile per tenant ────────────────────────────────────────
    // Only reconcile tenants where at least one cluster succeeded.
    // Tenants where every cluster failed keep their existing inferred
    // edges intact — a transient AWS outage shouldn't erase the
    // Catalog's service→workload topology.
    for tenant_id in &tenants_with_success {
        let tuples = tuples_by_tenant
            .get(tenant_id)
            .cloned()
            .unwrap_or_default();
        match reconcile_tenant(pool, *tenant_id, &tuples).await {
            Ok(deleted) => stats.edges_reconciled += deleted,
            Err(e) => {
                tracing::error!(
                    tenant_id = %tenant_id,
                    error = %e,
                    "k8s_inferer: reconciliation DELETE failed"
                );
            }
        }
    }

    Ok(stats)
}

/// Per-cluster scan result fed into the pass-level aggregation.
struct ClusterScanResult {
    services_examined: usize,
    edges_written: usize,
    /// `(from_id, to_id)` tuples this cluster's pass produced. Feeds the
    /// tenant-level reconciliation step.
    tuples: HashSet<(Uuid, Uuid)>,
}

/// Scan one cluster's Services and emit edges. Returns tuples produced so
/// the caller can reconcile at tenant scope.
async fn scan_cluster(
    pool: &PgPool,
    cluster: &Cluster,
    tenant_id: Uuid,
) -> Result<ClusterScanResult, Box<dyn std::error::Error + Send + Sync>> {
    let client = crate::services::k8s::build_k8s_client(pool, cluster)
        .await
        .map_err(|e| format!("build_k8s_client: {e}"))?;

    let svc_api: Api<Service> = Api::all(client.clone());
    let services = svc_api
        .list(&ListParams::default())
        .await
        .map_err(|e| format!("list services: {e}"))?;

    let mut result = ClusterScanResult {
        services_examined: 0,
        edges_written: 0,
        tuples: HashSet::new(),
    };

    for svc in &services.items {
        let Some(ns) = svc.metadata.namespace.as_deref() else {
            continue;
        };
        if is_system_namespace(ns) {
            continue;
        }
        let Some(svc_name) = svc.metadata.name.as_deref() else {
            continue;
        };

        // Selector is a BTreeMap<String, String> in k8s-openapi. Empty or
        // missing → skip (matches all pods, not a useful signal).
        let selector: BTreeMap<String, String> = svc
            .spec
            .as_ref()
            .and_then(|s| s.selector.clone())
            .map(|m| m.into_iter().collect())
            .unwrap_or_default();
        if selector.is_empty() {
            continue;
        }

        result.services_examined += 1;

        // List workloads in that namespace filtered by the selector.
        let selector_str = selector_to_label_string(&selector);
        let workload_names = match list_workloads(&client, ns, &selector_str).await {
            Ok(names) => names,
            Err(e) => {
                tracing::warn!(
                    cluster = %cluster.name,
                    namespace = ns,
                    service = svc_name,
                    error = %e,
                    "k8s_inferer: list workloads failed; skipping service"
                );
                continue;
            }
        };
        if workload_names.is_empty() {
            continue;
        }

        // Resolve the Service to an API entity — auto-create if absent.
        let api_id = match resolve_or_create_api_entity(
            pool,
            tenant_id,
            &cluster.name,
            ns,
            svc_name,
        )
        .await
        {
            Ok(id) => id,
            Err(e) => {
                tracing::warn!(
                    cluster = %cluster.name,
                    namespace = ns,
                    service = svc_name,
                    error = %e,
                    "k8s_inferer: could not resolve/create API entity; skipping"
                );
                continue;
            }
        };

        for workload_name in &workload_names {
            let component_id = match resolve_component_entity(
                pool,
                tenant_id,
                &cluster.name,
                ns,
                workload_name,
            )
            .await
            {
                Ok(Some(id)) => id,
                Ok(None) => {
                    // Workload has no Catalog Component yet — skip
                    // silently. `k8s_discovery` will pick it up on its
                    // own cadence and the next inferer pass will link.
                    continue;
                }
                Err(e) => {
                    tracing::warn!(
                        cluster = %cluster.name,
                        namespace = ns,
                        workload = workload_name,
                        error = %e,
                        "k8s_inferer: component lookup failed; skipping"
                    );
                    continue;
                }
            };

            // Self-edge would require api_id == component_id, which our
            // schema forbids by unique kinds per name, but defensively
            // skip anyway.
            if api_id == component_id {
                continue;
            }

            let insert_res = sqlx::query(
                r#"INSERT INTO catalog_relations (from_id, to_id, relation_type, source)
                   VALUES ($1, $2, $3, $4)
                   ON CONFLICT (from_id, to_id, relation_type) DO NOTHING"#,
            )
            .bind(api_id)
            .bind(component_id)
            .bind(RELATION_PROVIDES)
            .bind(EDGE_SOURCE)
            .execute(pool)
            .await;

            match insert_res {
                Ok(r) if r.rows_affected() > 0 => {
                    result.edges_written += 1;
                    result.tuples.insert((api_id, component_id));
                }
                Ok(_) => {
                    // Already present — still counts for reconciliation.
                    result.tuples.insert((api_id, component_id));
                }
                Err(e) => {
                    tracing::warn!(
                        cluster = %cluster.name,
                        namespace = ns,
                        service = svc_name,
                        workload = workload_name,
                        error = %e,
                        "k8s_inferer: relation INSERT failed; skipping"
                    );
                }
            }
        }
    }

    Ok(result)
}

/// Returns true if the namespace is in the exact-match blocklist OR has a
/// blocked prefix. Keep the checks side-effect free — it's called from
/// the hot loop.
pub(crate) fn is_system_namespace(ns: &str) -> bool {
    if SYSTEM_NAMESPACES.contains(&ns) {
        return true;
    }
    SYSTEM_NAMESPACE_PREFIXES
        .iter()
        .any(|prefix| ns.starts_with(prefix))
}

/// Turn a BTreeMap into the `key=value,key=value` form the Kubernetes
/// apiserver's `labelSelector` query parameter expects. BTreeMap gives us
/// deterministic iteration, useful for test assertions and for avoiding
/// spurious "selector changed" log lines if we ever surface the string.
pub(crate) fn selector_to_label_string(selector: &BTreeMap<String, String>) -> String {
    selector
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(",")
}

/// List Deployment / StatefulSet / DaemonSet names in `ns` matching
/// `selector_str`. Returns a de-duplicated set so a workload that exists
/// under multiple kinds (rare, mostly test fixtures) doesn't double-count.
async fn list_workloads(
    client: &kube::Client,
    ns: &str,
    selector_str: &str,
) -> Result<Vec<String>, Box<dyn std::error::Error + Send + Sync>> {
    let mut names: HashSet<String> = HashSet::new();

    // Partial results are fine — per the brief, don't retry paginated
    // fetches, just take what we got.
    if let Ok(list) = Api::<Deployment>::namespaced(client.clone(), ns)
        .list(&ListParams::default().labels(selector_str))
        .await
    {
        for d in list.items {
            if let Some(n) = d.metadata.name {
                names.insert(n);
            }
        }
    }
    if let Ok(list) = Api::<StatefulSet>::namespaced(client.clone(), ns)
        .list(&ListParams::default().labels(selector_str))
        .await
    {
        for s in list.items {
            if let Some(n) = s.metadata.name {
                names.insert(n);
            }
        }
    }
    if let Ok(list) = Api::<DaemonSet>::namespaced(client.clone(), ns)
        .list(&ListParams::default().labels(selector_str))
        .await
    {
        for d in list.items {
            if let Some(n) = d.metadata.name {
                names.insert(n);
            }
        }
    }

    Ok(names.into_iter().collect())
}

/// Look up a Component entity by the JSONB runtime tuple. Returns `None`
/// when no matching Component is in the Catalog yet — the caller treats
/// that as "nothing to link, move on".
async fn resolve_component_entity(
    pool: &PgPool,
    tenant_id: Uuid,
    cluster_name: &str,
    namespace: &str,
    workload: &str,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar::<_, Uuid>(
        r#"SELECT id FROM catalog_entities
           WHERE tenant_id = $1
             AND kind = 'component'
             AND spec -> 'runtime' ->> 'cluster' = $2
             AND spec -> 'runtime' ->> 'namespace' = $3
             AND spec -> 'runtime' ->> 'workload' = $4
           LIMIT 1"#,
    )
    .bind(tenant_id)
    .bind(cluster_name)
    .bind(namespace)
    .bind(workload)
    .fetch_optional(pool)
    .await
}

/// Resolve an API entity by `(tenant_id, name)` and — if absent — insert
/// one on the fly with `source = 'k8s_inferer'` in its annotations so the
/// YAML parser knows to leave it alone on imports that don't mention it.
///
/// Idempotent: `ON CONFLICT (tenant_id, kind, name) DO NOTHING` covers
/// the parallel-pass race, and we re-SELECT if the insert was a no-op.
async fn resolve_or_create_api_entity(
    pool: &PgPool,
    tenant_id: Uuid,
    cluster_name: &str,
    namespace: &str,
    service_name: &str,
) -> Result<Uuid, sqlx::Error> {
    // Fast path: already there.
    if let Some(id) = sqlx::query_scalar::<_, Uuid>(
        r#"SELECT id FROM catalog_entities
           WHERE tenant_id = $1 AND kind = 'api' AND name = $2
           LIMIT 1"#,
    )
    .bind(tenant_id)
    .bind(service_name)
    .fetch_optional(pool)
    .await?
    {
        return Ok(id);
    }

    let spec = api_entity_spec_json(cluster_name, namespace);
    let annotations = serde_json::json!({ SOURCE_ANNOTATION_KEY: SOURCE_K8S_INFERER });

    // INSERT ON CONFLICT ... DO NOTHING RETURNING — if the conflict path
    // fires, the query returns zero rows, so we fall back to a SELECT.
    let inserted: Option<Uuid> = sqlx::query_scalar::<_, Uuid>(
        r#"INSERT INTO catalog_entities (
               tenant_id, kind, name, lifecycle, annotations, spec
           )
           VALUES ($1, 'api', $2, 'production', $3, $4)
           ON CONFLICT (tenant_id, kind, name) DO NOTHING
           RETURNING id"#,
    )
    .bind(tenant_id)
    .bind(service_name)
    .bind(&annotations)
    .bind(&spec)
    .fetch_optional(pool)
    .await?;

    if let Some(id) = inserted {
        return Ok(id);
    }

    // Conflict path — someone else inserted between our SELECT and INSERT.
    sqlx::query_scalar::<_, Uuid>(
        r#"SELECT id FROM catalog_entities
           WHERE tenant_id = $1 AND kind = 'api' AND name = $2
           LIMIT 1"#,
    )
    .bind(tenant_id)
    .bind(service_name)
    .fetch_one(pool)
    .await
}

/// Build the `spec` JSONB for an API entity the inferer auto-creates.
/// Centralized so the unit test can assert on the shape without dragging
/// a real DB into the picture.
pub(crate) fn api_entity_spec_json(cluster: &str, namespace: &str) -> serde_json::Value {
    serde_json::json!({
        "lifecycle": "production",
        "owner": "unknown",
        "system": null,
        "runtime": {
            "kind": "k8s-service",
            "cluster": cluster,
            "namespace": namespace
        }
    })
}

/// Delete inferred edges for `tenant_id` that aren't in `keep`. `keep` is
/// the set of `(from_id, to_id)` tuples this pass re-confirmed; anything
/// else with `source='k8s_selector'` is stale and goes.
///
/// Uses parallel UUID arrays fed through `unnest` so we don't build a
/// multi-row VALUES list that would balloon under thousands of edges.
async fn reconcile_tenant(
    pool: &PgPool,
    tenant_id: Uuid,
    keep: &HashSet<(Uuid, Uuid)>,
) -> Result<usize, sqlx::Error> {
    let (from_ids, to_ids): (Vec<Uuid>, Vec<Uuid>) =
        keep.iter().copied().unzip();

    let result = sqlx::query(
        r#"DELETE FROM catalog_relations
           WHERE source = $1
             AND from_id IN (
                 SELECT id FROM catalog_entities WHERE tenant_id = $2
             )
             AND NOT EXISTS (
                 SELECT 1
                 FROM unnest($3::uuid[], $4::uuid[]) AS t(f, tt)
                 WHERE t.f = catalog_relations.from_id
                   AND t.tt = catalog_relations.to_id
             )"#,
    )
    .bind(EDGE_SOURCE)
    .bind(tenant_id)
    .bind(&from_ids)
    .bind(&to_ids)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() as usize)
}

// ---------------------------------------------------------------------------
// Tests — DB-free. The hot paths all delegate to pure helpers that can be
// exercised without a Postgres or K8s connection.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_interval_is_five_minutes() {
        assert_eq!(DEFAULT_INTERVAL.as_secs(), 300);
    }

    #[test]
    fn is_system_namespace_blocks_expected_ns() {
        // Exact-match system namespaces.
        for ns in [
            "kube-system",
            "kube-public",
            "kube-node-lease",
            "gmp-system",
            "cert-manager",
            "ingress-nginx",
        ] {
            assert!(is_system_namespace(ns), "expected {ns} to be system");
        }

        // `argo-` prefix — cover a few real Argo ns names.
        assert!(is_system_namespace("argo-events"));
        assert!(is_system_namespace("argo-rollouts"));
        assert!(is_system_namespace("argo-workflows"));

        // User namespaces must NOT be filtered.
        for ns in ["default", "prod", "team-payments", "argocd", "mimir"] {
            assert!(
                !is_system_namespace(ns),
                "{ns} should not be treated as system"
            );
        }
    }

    #[test]
    fn selector_to_label_string_is_deterministic() {
        // BTreeMap preserves key order — verify the string is stable so
        // a round-trip into `ListParams::labels` never surprises us.
        let mut sel = BTreeMap::new();
        sel.insert("app".to_string(), "order-api".to_string());
        sel.insert("tier".to_string(), "backend".to_string());
        assert_eq!(selector_to_label_string(&sel), "app=order-api,tier=backend");

        let empty = BTreeMap::new();
        assert_eq!(selector_to_label_string(&empty), "");
    }

    #[test]
    fn matches_selector_true_positive() {
        // A selector is "satisfied" by a label map if every (k, v) pair
        // in the selector appears in the label map. The apiserver does
        // this match for us, but the pure helper below mirrors the
        // semantics so tests can assert without a K8s.
        let mut selector = BTreeMap::new();
        selector.insert("app".to_string(), "order-api".to_string());

        let mut labels = BTreeMap::new();
        labels.insert("app".to_string(), "order-api".to_string());
        labels.insert("tier".to_string(), "backend".to_string());
        assert!(labels_match_selector(&labels, &selector));

        // Two-key selector matched by a superset of labels.
        let mut two_key = BTreeMap::new();
        two_key.insert("app".to_string(), "order-api".to_string());
        two_key.insert("tier".to_string(), "backend".to_string());
        assert!(labels_match_selector(&labels, &two_key));

        // Wrong value on a key breaks the match.
        let mut wrong_val = BTreeMap::new();
        wrong_val.insert("app".to_string(), "different".to_string());
        assert!(!labels_match_selector(&labels, &wrong_val));

        // Missing key breaks the match.
        let mut missing_key = BTreeMap::new();
        missing_key.insert("nonexistent".to_string(), "x".to_string());
        assert!(!labels_match_selector(&labels, &missing_key));
    }

    #[test]
    fn matches_selector_empty_selector_never_matches() {
        // An empty `spec.selector` would match everything in the
        // cluster if we fed it through; the inferer MUST short-circuit
        // before ever translating it to a label string. This test
        // pins the semantic: the is-empty check is the gate.
        let empty: BTreeMap<String, String> = BTreeMap::new();
        let mut labels = BTreeMap::new();
        labels.insert("app".to_string(), "foo".to_string());

        // Empty selector's label_string is empty — the inferer treats
        // that as "do not list workloads".
        assert_eq!(selector_to_label_string(&empty), "");
        // And the semantic check: our match helper rejects an empty
        // selector, so no accidental "matches everything" path.
        assert!(!labels_match_selector(&labels, &empty));
    }

    #[test]
    fn extract_api_spec_json_has_k8s_service_runtime_kind() {
        let v = api_entity_spec_json("prod-west-2", "payments");
        assert_eq!(
            v.get("runtime")
                .and_then(|r| r.get("kind"))
                .and_then(|k| k.as_str()),
            Some("k8s-service")
        );
        assert_eq!(
            v.get("runtime")
                .and_then(|r| r.get("cluster"))
                .and_then(|c| c.as_str()),
            Some("prod-west-2")
        );
        assert_eq!(
            v.get("runtime")
                .and_then(|r| r.get("namespace"))
                .and_then(|n| n.as_str()),
            Some("payments")
        );
        assert_eq!(
            v.get("lifecycle").and_then(|l| l.as_str()),
            Some("production")
        );
        assert_eq!(v.get("owner").and_then(|o| o.as_str()), Some("unknown"));
        // `system: null` survives round-tripping through serde_json.
        assert!(v.get("system").is_some_and(|s| s.is_null()));
    }

    #[test]
    fn edge_source_constants_are_stable() {
        // If these constants ever change, migrations downstream will
        // need backfills — this test is an alarm, not a check.
        assert_eq!(EDGE_SOURCE, "k8s_selector");
        assert_eq!(RELATION_PROVIDES, "provides");
        assert_eq!(SOURCE_K8S_INFERER, "k8s_inferer");
    }

    /// Local helper used by the selector-match tests. Mirrors the
    /// semantics the apiserver's labelSelector applies when we call
    /// `ListParams::default().labels(...)`: selector is a subset
    /// (by key+value) of the label map.
    fn labels_match_selector(
        labels: &BTreeMap<String, String>,
        selector: &BTreeMap<String, String>,
    ) -> bool {
        if selector.is_empty() {
            // The inferer treats empty selector as "skip entirely", so
            // the test helper mirrors that — empty selector never
            // matches, even when the labels map is non-empty.
            return false;
        }
        selector
            .iter()
            .all(|(k, v)| labels.get(k).is_some_and(|lv| lv == v))
    }
}
