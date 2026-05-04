//! Aggregator for the services v2 overview endpoint.
//!
//! This module fuses five data sources into a single response payload:
//!
//! 1. `catalog_entities` (kind='component' plus kind='system' for grouping)
//! 2. `slos`
//! 3. `error_budget_snapshots` (latest per slo_id — LATERAL join)
//! 4. `slo_burn_events` (active / unresolved only)
//! 5. `incidents` (status != 'closed' and affected_component_ids overlaps
//!    with our components)
//!
//! Performance contract (design §3.3, acceptance criterion #2): <500ms for
//! a 100-Component tenant. We do **batch queries** (one per data source)
//! and stitch them together in-memory with HashMaps keyed by component_id /
//! slo_id. Probes are DB-only (see `runtime_probe.rs`) so the only real
//! hot path is the SELECTs; we avoid per-component round-trips entirely.
//!
//! Total SQL queries per `build_overview` call:
//!
//!   1. components list
//!   2. systems list (for grouping metadata)
//!   3. slos list (scoped to component_ids we just fetched)
//!   4. latest error_budget_snapshot per slo
//!   5. unresolved slo_burn_events per slo
//!   6. active incidents with any affected_component_ids overlap
//!   7. (per component) runtime probe — mostly 0 extra queries; EKS does
//!      one cluster lookup + one deployment_events lookup
//!
//! So: 6 flat queries + up to 2 × N_eks_components. With 100 Components
//! where half are EKS that's ~106 queries; all are indexed single-row
//! reads so the total stays well under the 500ms budget.

use std::collections::HashMap;

use sqlx::PgPool;
use uuid::Uuid;

use crate::error::AppError;
use crate::models::catalog::{CatalogEntity, KIND_COMPONENT, KIND_SYSTEM};
use crate::models::incident::Incident;
use crate::models::services_view::{
    ComponentOverview, HealthCounts, HealthStatus, RuntimeSpec, ServicesOverviewResponse,
    SloSummary, SystemSummary,
};
use crate::models::slo::{ErrorBudgetSnapshot, Slo};

use super::health::compute_health;
use super::runtime_probe;

/// Assembled per-SLO state — internal scratch type, not a public DTO.
#[derive(Default)]
struct SloState {
    budget_remaining_pct: Option<f64>,
    burn_rate_1h: Option<f64>,
}

/// Build the full overview response for the tenant scope implied by
/// `tenant_filter`. Pass `None` to skip tenant scoping (super_admin view).
pub async fn build_overview(
    pool: &PgPool,
    tenant_filter: Option<Uuid>,
) -> Result<ServicesOverviewResponse, AppError> {
    // ---------------------------------------------------------------------
    // 1. Components
    // ---------------------------------------------------------------------
    let components: Vec<CatalogEntity> = if let Some(tid) = tenant_filter {
        sqlx::query_as::<_, CatalogEntity>(
            r#"SELECT * FROM catalog_entities
               WHERE kind = $1 AND tenant_id = $2
               ORDER BY name ASC"#,
        )
        .bind(KIND_COMPONENT)
        .bind(tid)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as::<_, CatalogEntity>(
            r#"SELECT * FROM catalog_entities
               WHERE kind = $1
               ORDER BY name ASC"#,
        )
        .bind(KIND_COMPONENT)
        .fetch_all(pool)
        .await?
    };

    let component_ids: Vec<Uuid> = components.iter().map(|c| c.id).collect();

    // ---------------------------------------------------------------------
    // 2. Systems (for grouping metadata). Still scoped by tenant when
    //    caller asked for it.
    // ---------------------------------------------------------------------
    let systems: Vec<CatalogEntity> = if let Some(tid) = tenant_filter {
        sqlx::query_as::<_, CatalogEntity>(
            r#"SELECT * FROM catalog_entities
               WHERE kind = $1 AND tenant_id = $2
               ORDER BY name ASC"#,
        )
        .bind(KIND_SYSTEM)
        .bind(tid)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as::<_, CatalogEntity>(
            r#"SELECT * FROM catalog_entities
               WHERE kind = $1
               ORDER BY name ASC"#,
        )
        .bind(KIND_SYSTEM)
        .fetch_all(pool)
        .await?
    };

    // ---------------------------------------------------------------------
    // 3. SLOs (scoped to the components we just fetched).
    // ---------------------------------------------------------------------
    let slos: Vec<Slo> = if component_ids.is_empty() {
        vec![]
    } else {
        sqlx::query_as::<_, Slo>(
            r#"SELECT * FROM slos
               WHERE component_id = ANY($1)"#,
        )
        .bind(&component_ids)
        .fetch_all(pool)
        .await?
    };

    let slo_ids: Vec<Uuid> = slos.iter().map(|s| s.id).collect();

    // ---------------------------------------------------------------------
    // 4. Latest ErrorBudgetSnapshot per SLO via LATERAL join.
    //    Same pattern as `slo::budgets_batch` — one round-trip for N slos.
    // ---------------------------------------------------------------------
    let snapshots: Vec<ErrorBudgetSnapshot> = if slo_ids.is_empty() {
        vec![]
    } else {
        sqlx::query_as::<_, ErrorBudgetSnapshot>(
            r#"SELECT ebs.*
               FROM slos s
               JOIN LATERAL (
                   SELECT *
                   FROM error_budget_snapshots
                   WHERE slo_id = s.id
                   ORDER BY captured_at DESC
                   LIMIT 1
               ) ebs ON TRUE
               WHERE s.id = ANY($1)"#,
        )
        .bind(&slo_ids)
        .fetch_all(pool)
        .await?
    };

    // ---------------------------------------------------------------------
    // 5. Active (unresolved) burn events per SLO. We take the max
    //    burn_rate per slo_id client-side.
    // ---------------------------------------------------------------------
    let burn_rows: Vec<(Uuid, f64)> = if slo_ids.is_empty() {
        vec![]
    } else {
        sqlx::query_as(
            r#"SELECT slo_id, burn_rate
               FROM slo_burn_events
               WHERE resolved_at IS NULL
                 AND slo_id = ANY($1)
                 AND burn_window = '1h'"#,
        )
        .bind(&slo_ids)
        .fetch_all(pool)
        .await?
    };

    // ---------------------------------------------------------------------
    // 6. Active incidents touching any of our components. One query with
    //    GIN-index-friendly `&&` array overlap.
    // ---------------------------------------------------------------------
    let incidents: Vec<Incident> = if component_ids.is_empty() {
        vec![]
    } else {
        // Tenant scoping: when tenant_filter is set, restrict to that
        // tenant — otherwise (super_admin) include all. We still apply
        // the array overlap to keep the result small.
        if let Some(tid) = tenant_filter {
            sqlx::query_as::<_, Incident>(
                r#"SELECT * FROM incidents
                   WHERE status <> 'closed'
                     AND tenant_id = $1
                     AND affected_component_ids && $2::uuid[]"#,
            )
            .bind(tid)
            .bind(&component_ids)
            .fetch_all(pool)
            .await?
        } else {
            sqlx::query_as::<_, Incident>(
                r#"SELECT * FROM incidents
                   WHERE status <> 'closed'
                     AND affected_component_ids && $1::uuid[]"#,
            )
            .bind(&component_ids)
            .fetch_all(pool)
            .await?
        }
    };

    // ---------------------------------------------------------------------
    // In-memory joins
    // ---------------------------------------------------------------------

    // slo_id -> (budget%, burn_rate_1h).
    let mut slo_state: HashMap<Uuid, SloState> = HashMap::new();
    for snap in &snapshots {
        let entry = slo_state.entry(snap.slo_id).or_default();
        entry.budget_remaining_pct = Some(snap.budget_remaining_pct);
        // burn_rate_1h on the snapshot is populated by the snapshot runner
        // but the authoritative "currently firing" 1h burn lives in
        // slo_burn_events. Use whichever is larger so the card never
        // under-reports.
        entry.burn_rate_1h = max_f64(entry.burn_rate_1h, snap.burn_rate_1h);
    }
    for (sid, rate) in burn_rows {
        let entry = slo_state.entry(sid).or_default();
        entry.burn_rate_1h = max_f64(entry.burn_rate_1h, Some(rate));
    }

    // component_id -> Vec<SLO summary pieces>
    let mut slos_by_component: HashMap<Uuid, Vec<&Slo>> = HashMap::new();
    for slo in &slos {
        if let Some(cid) = slo.component_id {
            slos_by_component.entry(cid).or_default().push(slo);
        }
    }

    // component_id -> Vec<Incident>
    let mut incidents_by_component: HashMap<Uuid, Vec<Incident>> = HashMap::new();
    for inc in incidents {
        for cid in &inc.affected_component_ids {
            incidents_by_component
                .entry(*cid)
                .or_default()
                .push(inc.clone());
        }
    }

    // ---------------------------------------------------------------------
    // 7. Per-component assemble
    // ---------------------------------------------------------------------
    let mut overviews: Vec<ComponentOverview> = Vec::with_capacity(components.len());
    for c in &components {
        let ov = assemble_one(
            pool,
            c,
            slos_by_component.get(&c.id).map(|v| v.as_slice()).unwrap_or(&[]),
            &slo_state,
            incidents_by_component.get(&c.id).cloned().unwrap_or_default(),
        )
        .await;
        overviews.push(ov);
    }

    // ---------------------------------------------------------------------
    // 8. Roll up into SystemSummary
    // ---------------------------------------------------------------------
    let mut system_counts: HashMap<Uuid, HealthCounts> = HashMap::new();
    let mut system_component_counts: HashMap<Uuid, i64> = HashMap::new();
    for ov in &overviews {
        if let Some(sid) = ov.system_id {
            let counts = system_counts.entry(sid).or_default();
            match ov.health {
                HealthStatus::Healthy => counts.healthy += 1,
                HealthStatus::Warning => counts.warning += 1,
                HealthStatus::Critical => counts.critical += 1,
                HealthStatus::Unknown => counts.unknown += 1,
            }
            *system_component_counts.entry(sid).or_default() += 1;
        }
    }

    let mut system_summaries: Vec<SystemSummary> = systems
        .iter()
        .filter_map(|sys| {
            let count = *system_component_counts.get(&sys.id).unwrap_or(&0);
            if count == 0 {
                // Don't emit empty Systems — the frontend doesn't render
                // a group header with zero children anyway, and this
                // keeps the payload small.
                return None;
            }
            Some(SystemSummary {
                id: sys.id,
                name: sys.name.clone(),
                display_name: sys.display_name.clone(),
                component_count: count,
                health_summary: system_counts.get(&sys.id).copied().unwrap_or_default(),
            })
        })
        .collect();

    // Sort Systems so the UI order is stable (Critical first, then
    // alphabetical). We sort by a simple tuple: descending critical count,
    // then descending warning count, then name asc.
    system_summaries.sort_by(|a, b| {
        b.health_summary
            .critical
            .cmp(&a.health_summary.critical)
            .then(b.health_summary.warning.cmp(&a.health_summary.warning))
            .then(a.name.cmp(&b.name))
    });

    Ok(ServicesOverviewResponse {
        systems: system_summaries,
        components: overviews,
    })
}

/// Build a single ComponentOverview. Called in a loop over all components
/// but does **no queries** other than the runtime probe — which itself is
/// at most two indexed single-row reads (EKS path) or zero queries for
/// all other kinds.
async fn assemble_one(
    pool: &PgPool,
    entity: &CatalogEntity,
    slos_for_c: &[&Slo],
    slo_state: &HashMap<Uuid, SloState>,
    incidents: Vec<Incident>,
) -> ComponentOverview {
    // Parse runtime from spec.runtime — tolerates missing / mis-shaped
    // payloads by treating them as None.
    let runtime: Option<RuntimeSpec> = entity
        .spec
        .get("runtime")
        .and_then(|v| serde_json::from_value::<RuntimeSpec>(v.clone()).ok());

    let runtime_detail = runtime_probe::probe(pool, runtime.as_ref()).await;

    // SLO rollup: total = count; min budget_remaining and max burn_rate
    // over all SLOs attached to this Component.
    let mut slo_summary = SloSummary {
        total: slos_for_c.len() as i64,
        ..SloSummary::default()
    };
    for slo in slos_for_c {
        if let Some(state) = slo_state.get(&slo.id) {
            if let Some(pct) = state.budget_remaining_pct {
                slo_summary.budget_remaining_min_pct = Some(
                    slo_summary
                        .budget_remaining_min_pct
                        .map(|cur| cur.min(pct))
                        .unwrap_or(pct),
                );
            }
            if let Some(rate) = state.burn_rate_1h {
                slo_summary.burn_rate_1h_max = Some(
                    slo_summary
                        .burn_rate_1h_max
                        .map(|cur| cur.max(rate))
                        .unwrap_or(rate),
                );
            }
        }
    }

    let active_incident_count = incidents.len() as i64;
    let (health, health_reason) =
        compute_health(&slo_summary, &incidents, &runtime_detail);

    ComponentOverview {
        id: entity.id,
        name: entity.name.clone(),
        display_name: entity.display_name.clone(),
        description: entity.description.clone(),
        lifecycle: entity.lifecycle.clone(),
        system_id: entity.system_id,
        owner_group_id: entity.owner_group_id,
        tags: entity.tags.clone(),
        runtime,
        runtime_detail,
        health,
        health_reason,
        active_incident_count,
        slo_summary,
    }
}

/// Nullable max helper — `Option::max` doesn't do the right thing for
/// `None` (it returns `None` when either side is None). We want "Some wins
/// over None, larger value wins when both are Some".
fn max_f64(a: Option<f64>, b: Option<f64>) -> Option<f64> {
    match (a, b) {
        (Some(x), Some(y)) => Some(x.max(y)),
        (Some(x), None) => Some(x),
        (None, Some(y)) => Some(y),
        (None, None) => None,
    }
}

// ---------------------------------------------------------------------------
// Tests — pure helpers only. DB-touching `build_overview` is covered by
// integration tests (deferred per unit scope).
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn max_f64_prefers_larger_and_handles_nones() {
        assert_eq!(max_f64(None, None), None);
        assert_eq!(max_f64(Some(1.0), None), Some(1.0));
        assert_eq!(max_f64(None, Some(2.0)), Some(2.0));
        assert_eq!(max_f64(Some(1.0), Some(2.0)), Some(2.0));
        assert_eq!(max_f64(Some(5.0), Some(2.0)), Some(5.0));
    }
}
