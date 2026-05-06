//! K8s Service selector → workload relation inference.
//!
//! **Status**: stub scaffold (Phase 1 prep commit).
//! The real implementation is landing in the Agent A follow-up commit —
//! see the dispatch brief in the PR description.
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
//! ## Contract for Agent A
//!
//! See `aidlc-docs/2026-05-06-phase1-multi-module/agent-a-brief.md` for the
//! full checklist. High-level steps on each tick:
//!
//!   1. `SELECT id, tenant_id, name FROM clusters WHERE status = 'active'`
//!   2. For each cluster: `services::k8s::build_k8s_client(cluster)`
//!      (this already handles EKS vs GKE auth — see `services/k8s.rs` +
//!      `services/gcp.rs`).
//!   3. List Services (all non-system namespaces). System-ns filter list is
//!      the same one used by the `/topology` page — see
//!      `frontend/components/topology/*` for the canonical list (`kube-system`,
//!      `gmp-system`, `argo-*`, `cert-manager`, `ingress-nginx`, …).
//!   4. For each Service, read `spec.selector`; list workloads matching
//!      that selector via `label_selector()` on the `kube` List API.
//!   5. Resolve workload → `catalog_entities.id` (Component) by
//!      matching `spec_json -> 'runtime' ->> 'cluster' = cluster.name`
//!      AND `namespace = ns` AND `workload = workload_name`.
//!      *Important*: use the `spec_json` JSONB path (Components store
//!      runtime as a nested object — see `services/services_view/aggregator.rs`).
//!   6. Resolve Service → `catalog_entities.id` (API kind) by
//!      matching by Service name + namespace. Components with `kind='api'`
//!      that declare the same `spec.runtime.workload` are a close second
//!      fallback when no API entity is declared.
//!   7. `INSERT INTO catalog_relations (from_id, to_id, relation_type, source)
//!      VALUES ($api_id, $component_id, 'provides', 'k8s_selector')
//!      ON CONFLICT (from_id, to_id, relation_type) DO NOTHING`.
//!      (Patterns to copy: `services/catalog/yaml_parser.rs` INSERT block.)
//!   8. Reconciliation pass — delete stale inferred edges:
//!      ```sql
//!      DELETE FROM catalog_relations
//!        WHERE source = 'k8s_selector'
//!          AND tenant_scoped_cluster_predicate(...)
//!          AND (from_id, to_id) NOT IN (current_pass_tuples)
//!      ```
//!      Tenant isolation is REQUIRED — see `CLAUDE.md` §Access Control.
//!
//! ## Patterns to reuse
//!
//! | Concern              | File to mirror                                   |
//! |----------------------|--------------------------------------------------|
//! | K8s client build     | `backend/src/services/k8s.rs::build_k8s_client`  |
//! | Loop shape           | `backend/src/services/slo/snapshot_runner.rs`    |
//! | Relations INSERT     | `backend/src/services/catalog/yaml_parser.rs`    |
//! | Multi-tenancy        | any handler under `backend/src/handlers/`        |
//! | Namespace exclusions | `frontend/components/topology/` (canonical list) |
//!
//! ## Error semantics
//!
//! Per-cluster failures (network, auth, CRD missing) MUST log at `warn` and
//! continue the loop; one bad cluster cannot block the others. Any
//! unexpected DB error (constraint violation, connection loss) should bubble
//! out of the per-cluster call so the caller logs the stack but the outer
//! loop survives.

use sqlx::PgPool;
use std::time::Duration;

/// Interval between scan passes. 5 minutes balances freshness against
/// K8s API pressure; override with `K8S_INFERER_INTERVAL_SECS` env var.
pub const DEFAULT_INTERVAL: Duration = Duration::from_secs(300);

/// Background loop. Spawned once at startup from `main.rs`. Runs forever;
/// panics inside tasks are caught by the tokio runtime's default hook.
///
/// Agent A: replace this body with the real implementation. Keep the
/// function signature stable — `main.rs` already wires this entry point.
pub async fn run_inferer_loop(pool: PgPool) {
    let interval = std::env::var("K8S_INFERER_INTERVAL_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or(DEFAULT_INTERVAL);

    tracing::info!(
        "k8s_inferer scaffold online (interval = {}s). Real inference lands in Agent A commit.",
        interval.as_secs()
    );

    // Touch the pool arg so the "unused" lint doesn't fire on the stub.
    let _ = &pool;

    loop {
        tokio::time::sleep(interval).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_interval_is_five_minutes() {
        assert_eq!(DEFAULT_INTERVAL.as_secs(), 300);
    }
}
