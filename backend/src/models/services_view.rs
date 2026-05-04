//! Services v2 view model — aggregated response for `GET /api/services/overview`.
//!
//! The overview endpoint fuses Catalog `component` entities with their SLO
//! budget state, active incident count, and a runtime-specific probe
//! detail shape so the front end can render the services grid in a single
//! round-trip (design §3.3, §4.1).
//!
//! All types here are response-only DTOs; the source-of-truth tables are
//! `catalog_entities`, `slos`, `error_budget_snapshots`, `slo_burn_events`,
//! `incidents`, `deployment_events`. See `services/services_view/` for the
//! aggregator and the pure-function health calculator.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Top-level response
// ---------------------------------------------------------------------------

/// Full payload returned by `GET /api/services/overview`. Every Component
/// the caller can see is emitted once in `components`; Systems are rolled
/// up into `systems` with per-health counts so the UI can render section
/// headers without re-scanning the component list.
#[derive(Debug, Serialize)]
pub struct ServicesOverviewResponse {
    pub systems: Vec<SystemSummary>,
    pub components: Vec<ComponentOverview>,
}

/// Rolled-up view of a Catalog `system` entity. `component_count` and
/// `health_summary` are computed from the `components` array, not queried
/// separately — the aggregator does the single pass so the counts always
/// match what the UI sees.
#[derive(Debug, Serialize)]
pub struct SystemSummary {
    pub id: Uuid,
    pub name: String,
    pub display_name: Option<String>,
    pub component_count: i64,
    pub health_summary: HealthCounts,
}

/// Per-health bucket counts for a System's components. All four states are
/// always present (zeroed when empty) so the front end can render the bar
/// without defensive null checks.
#[derive(Debug, Serialize, Default, Clone, Copy)]
pub struct HealthCounts {
    pub healthy: i64,
    pub warning: i64,
    pub critical: i64,
    pub unknown: i64,
}

// ---------------------------------------------------------------------------
// Per-component overview
// ---------------------------------------------------------------------------

/// One Catalog `component` with the runtime / SLO / incident context the
/// services grid needs to render a card. Field ordering mirrors the shape
/// documented in design §4.1 — keep it aligned so the client schema codegen
/// stays stable.
#[derive(Debug, Serialize)]
pub struct ComponentOverview {
    pub id: Uuid,
    pub name: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub lifecycle: String,
    pub system_id: Option<Uuid>,
    pub owner_group_id: Option<Uuid>,
    pub tags: Vec<String>,
    /// Declared runtime spec from `catalog_entities.spec.runtime`. `None`
    /// when the Component has no `runtime` stanza (legacy entries).
    pub runtime: Option<RuntimeSpec>,
    /// Runtime-specific probe result, tagged union discriminated by `kind`.
    /// v1: DB-only (reads cached deployment events / spec fields, no live
    /// AWS/K8s calls).
    pub runtime_detail: RuntimeDetail,
    pub health: HealthStatus,
    pub health_reason: String,
    pub active_incident_count: i64,
    pub slo_summary: SloSummary,
}

/// Declared runtime binding parsed from `spec.runtime` on a Component.
/// Fields are intentionally loose — different runtime kinds use different
/// subsets (EKS uses cluster/namespace/workload; Lambda uses arn/region;
/// External uses base_url/health_url; etc.).
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct RuntimeSpec {
    pub kind: String,
    #[serde(default)]
    pub cluster: Option<String>,
    #[serde(default)]
    pub namespace: Option<String>,
    #[serde(default)]
    pub workload: Option<String>,
    #[serde(default)]
    pub instance_id: Option<String>,
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default)]
    pub arn: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub health_url: Option<String>,
}

// ---------------------------------------------------------------------------
// Runtime-specific detail (tagged union)
// ---------------------------------------------------------------------------

/// Runtime-specific probe result. Serialised with an external `kind` tag so
/// the shape is flat in JSON (`{"kind":"eks","pod_ready":2,...}`).
///
/// `Unavailable` is returned when the probe can't get a usable answer — e.g.
/// EKS but no `deployment_events` rows, cluster unknown, or credentials
/// missing. The card still renders, just with `?` placeholders.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum RuntimeDetail {
    Eks {
        pod_ready: Option<i32>,
        pod_desired: Option<i32>,
        image: Option<String>,
        last_updated: Option<DateTime<Utc>>,
    },
    Ec2 {
        state: Option<String>,
        ami: Option<String>,
        instance_type: Option<String>,
    },
    Rds {
        engine: Option<String>,
        version: Option<String>,
        multi_az: Option<bool>,
        connection_count: Option<i32>,
    },
    Lambda {
        version: Option<String>,
        memory_mb: Option<i32>,
        last_invocation: Option<DateTime<Utc>>,
        error_rate_pct: Option<f64>,
    },
    External {
        base_url: Option<String>,
        last_rtt_ms: Option<i32>,
        last_check: Option<DateTime<Utc>>,
    },
    Generic {
        info: serde_json::Value,
    },
    /// Probe failed / cluster down / no credentials / no cached data.
    Unavailable {
        reason: String,
    },
}

impl RuntimeDetail {
    /// Stable discriminator string — tests use it to assert the right
    /// variant was picked by the probe dispatcher without unpacking the
    /// full payload.
    pub fn kind_tag(&self) -> &'static str {
        match self {
            RuntimeDetail::Eks { .. } => "eks",
            RuntimeDetail::Ec2 { .. } => "ec2",
            RuntimeDetail::Rds { .. } => "rds",
            RuntimeDetail::Lambda { .. } => "lambda",
            RuntimeDetail::External { .. } => "external",
            RuntimeDetail::Generic { .. } => "generic",
            RuntimeDetail::Unavailable { .. } => "unavailable",
        }
    }

    /// True when the probe could not collect usable runtime data. Used by
    /// the health calculator to force Critical.
    pub fn is_unavailable(&self) -> bool {
        matches!(self, RuntimeDetail::Unavailable { .. })
    }
}

// ---------------------------------------------------------------------------
// Health
// ---------------------------------------------------------------------------

/// Four-state health signal. `Unknown` is intentionally last so the frontend
/// sort order (Critical > Warning > Healthy > Unknown) matches the variant
/// ordinal when enumerated.
#[derive(Debug, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    Healthy,
    Warning,
    Critical,
    Unknown,
}

// ---------------------------------------------------------------------------
// SLO summary
// ---------------------------------------------------------------------------

/// Per-Component SLO rollup. `total` is SLO row count; the two min/max
/// fields are `None` when the Component has zero SLOs (so the card can
/// show `—` instead of `0.0`).
#[derive(Debug, Serialize, Default, Clone)]
pub struct SloSummary {
    pub total: i64,
    pub budget_remaining_min_pct: Option<f64>,
    pub burn_rate_1h_max: Option<f64>,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_detail_serialises_with_external_tag() {
        // External tag is `kind`. Confirm it lands in the JSON and that
        // fields merge flat with the tag — this is the contract the
        // frontend codegen relies on.
        let eks = RuntimeDetail::Eks {
            pod_ready: Some(2),
            pod_desired: Some(2),
            image: Some("acme/order-api:v1".into()),
            last_updated: None,
        };
        let v = serde_json::to_value(&eks).expect("serialize");
        assert_eq!(v["kind"], "eks");
        assert_eq!(v["pod_ready"], 2);
        assert_eq!(v["image"], "acme/order-api:v1");
    }

    #[test]
    fn runtime_detail_unavailable_round_trips_reason() {
        let u = RuntimeDetail::Unavailable {
            reason: "no deployment_events".into(),
        };
        let v = serde_json::to_value(&u).expect("serialize");
        assert_eq!(v["kind"], "unavailable");
        assert_eq!(v["reason"], "no deployment_events");
        assert!(u.is_unavailable());
        assert!(!RuntimeDetail::Generic {
            info: serde_json::Value::Null
        }
        .is_unavailable());
    }

    #[test]
    fn health_counts_default_is_all_zero() {
        let c = HealthCounts::default();
        assert_eq!(c.healthy, 0);
        assert_eq!(c.warning, 0);
        assert_eq!(c.critical, 0);
        assert_eq!(c.unknown, 0);
    }

    #[test]
    fn runtime_spec_parses_partial_fields() {
        // Realistic payload: only kind + EKS triple; rest default to None.
        let raw = r#"{ "kind": "eks", "cluster": "prod", "namespace": "payments", "workload": "order-api" }"#;
        let spec: RuntimeSpec = serde_json::from_str(raw).expect("parse");
        assert_eq!(spec.kind, "eks");
        assert_eq!(spec.cluster.as_deref(), Some("prod"));
        assert!(spec.arn.is_none());
        assert!(spec.base_url.is_none());
    }

    #[test]
    fn runtime_detail_kind_tags_cover_all_variants() {
        // Keep the tag list in sync with the enum so adding a variant
        // forces a test update — the frontend switch statement depends on
        // these exact strings.
        let variants = [
            RuntimeDetail::Eks {
                pod_ready: None,
                pod_desired: None,
                image: None,
                last_updated: None,
            },
            RuntimeDetail::Ec2 {
                state: None,
                ami: None,
                instance_type: None,
            },
            RuntimeDetail::Rds {
                engine: None,
                version: None,
                multi_az: None,
                connection_count: None,
            },
            RuntimeDetail::Lambda {
                version: None,
                memory_mb: None,
                last_invocation: None,
                error_rate_pct: None,
            },
            RuntimeDetail::External {
                base_url: None,
                last_rtt_ms: None,
                last_check: None,
            },
            RuntimeDetail::Generic {
                info: serde_json::Value::Null,
            },
            RuntimeDetail::Unavailable {
                reason: "x".into(),
            },
        ];
        let expected = [
            "eks",
            "ec2",
            "rds",
            "lambda",
            "external",
            "generic",
            "unavailable",
        ];
        for (v, tag) in variants.iter().zip(expected.iter()) {
            assert_eq!(v.kind_tag(), *tag);
        }
    }
}
