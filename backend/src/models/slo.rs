//! SLO engine types — W1 core data model.
//!
//! Mirrors the schema in `backend/src/migrations/20260502100001_slo_core.sql`.
//!
//! Three top-level entities:
//!
//! * [`Slo`]                  — a user-defined Service Level Objective:
//!                              SLI query pair + objective + rolling window.
//! * [`ErrorBudgetSnapshot`]  — 5-minute snapshot of budget/burn state.
//! * [`SloBurnEvent`]         — a Multi-Window Multi-Burn-Rate alert,
//!                              optionally linked to an [`Issue`].
//!
//! See `docs/platform-evolution.md` §4.2 (data model) and §4.4 (MWMBR).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// SLI type constants (mirrors the CHECK constraint on `slos.sli_type`).
// ---------------------------------------------------------------------------

pub const SLI_TYPE_AVAILABILITY: &str = "availability";
pub const SLI_TYPE_LATENCY: &str = "latency";
pub const SLI_TYPE_ERROR_RATE: &str = "error_rate";
pub const SLI_TYPE_CUSTOM: &str = "custom";

pub const ALL_SLI_TYPES: &[&str] = &[
    SLI_TYPE_AVAILABILITY,
    SLI_TYPE_LATENCY,
    SLI_TYPE_ERROR_RATE,
    SLI_TYPE_CUSTOM,
];

// ---------------------------------------------------------------------------
// Burn event severity (mirrors the CHECK on `slo_burn_events.severity`).
// ---------------------------------------------------------------------------

pub const BURN_SEVERITY_PAGE: &str = "page";
pub const BURN_SEVERITY_TICKET: &str = "ticket";

pub const ALL_BURN_SEVERITIES: &[&str] = &[BURN_SEVERITY_PAGE, BURN_SEVERITY_TICKET];

/// Window_days is constrained to these values by the DB CHECK constraint.
pub const ALLOWED_WINDOW_DAYS: &[i32] = &[7, 28, 30];

// ---------------------------------------------------------------------------
// SLO definition.
// ---------------------------------------------------------------------------

/// A user-defined Service Level Objective.
///
/// `good_events_query` / `total_events_query` are raw PromQL strings — we
/// store the query text rather than a structured DSL (see design §4.1
/// decision). `objective_pct` is expressed in human-readable percent
/// (e.g. `99.9`), not as a ratio.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Slo {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub component_id: Option<Uuid>,
    pub name: String,
    pub description: Option<String>,
    pub sli_type: String,
    pub good_events_query: String,
    pub total_events_query: String,
    pub objective_pct: f64,
    pub window_days: i32,
    pub burn_rate_policy: String,
    pub labels: serde_json::Value,
    pub enabled: bool,
    pub recording_rules_hash: Option<String>,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Slo {
    /// Returns true if the given `sli_type` is one of the four valid values.
    pub fn is_valid_sli_type(sli_type: &str) -> bool {
        ALL_SLI_TYPES.contains(&sli_type)
    }

    /// Returns true if the given `window_days` is allowed by the schema.
    pub fn is_valid_window_days(window_days: i32) -> bool {
        ALLOWED_WINDOW_DAYS.contains(&window_days)
    }
}

// ---------------------------------------------------------------------------
// Error budget snapshot.
// ---------------------------------------------------------------------------

/// One row in the 5-minute budget snapshot table. Burn-rate columns are
/// nullable because the MWMBR windows may not yet be populated (e.g. a
/// freshly-created SLO).
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ErrorBudgetSnapshot {
    pub id: Uuid,
    pub slo_id: Uuid,
    pub captured_at: DateTime<Utc>,
    pub window_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,
    pub sli_achieved_pct: f64,
    pub budget_total_minutes: f64,
    pub budget_consumed_minutes: f64,
    pub budget_remaining_pct: f64,
    pub burn_rate_1h: Option<f64>,
    pub burn_rate_6h: Option<f64>,
    pub burn_rate_24h: Option<f64>,
    pub burn_rate_3d: Option<f64>,
}

/// Compact per-SLO budget view used by the batch endpoint
/// `GET /api/slos/budgets?ids=...`. Returns the latest snapshot per SLO
/// without the window/row bookkeeping fields so the UI can paint a list
/// of burn cards with a single round-trip (P1 #18).
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct BudgetSummary {
    pub slo_id: Uuid,
    pub budget_remaining_pct: f64,
    pub budget_total_minutes: f64,
    pub budget_consumed_minutes: f64,
    pub burn_rate_1h: Option<f64>,
    pub burn_rate_6h: Option<f64>,
    pub sli_achieved_pct: f64,
    pub captured_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Burn event.
// ---------------------------------------------------------------------------

/// A single Multi-Window Multi-Burn-Rate alert firing. `window` is the alert
/// window (e.g. `"1h"`, `"6h"`, `"3d"`, `"7d"`); `severity` is the MWMBR
/// class (page / ticket).
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SloBurnEvent {
    pub id: Uuid,
    pub slo_id: Uuid,
    pub severity: String,
    pub window: String,
    pub burn_rate: f64,
    pub threshold: f64,
    pub triggered_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub issue_id: Option<Uuid>,
}

// ---------------------------------------------------------------------------
// Request DTOs (for W2 handlers; declared here so the model stays the single
// source of truth for field names / serde naming).
// ---------------------------------------------------------------------------

/// Payload for `POST /api/slos`.
///
/// Excludes server-generated fields: `id`, `created_at`, `updated_at`,
/// `recording_rules_hash`. `created_by` and `tenant_id` are injected by the
/// handler from the authenticated user's context.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreateSloRequest {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub component_id: Option<Uuid>,
    pub sli_type: String,
    pub good_events_query: String,
    pub total_events_query: String,
    pub objective_pct: f64,
    pub window_days: i32,
    #[serde(default = "default_burn_rate_policy")]
    pub burn_rate_policy: String,
    #[serde(default)]
    pub labels: serde_json::Value,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_burn_rate_policy() -> String {
    "mwmbr_default".to_string()
}

fn default_true() -> bool {
    true
}

/// Payload for `PATCH /api/slos/:id`. All fields optional — only supplied
/// fields will be updated.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct UpdateSloRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub component_id: Option<Uuid>,
    pub sli_type: Option<String>,
    pub good_events_query: Option<String>,
    pub total_events_query: Option<String>,
    pub objective_pct: Option<f64>,
    pub window_days: Option<i32>,
    pub burn_rate_policy: Option<String>,
    pub labels: Option<serde_json::Value>,
    pub enabled: Option<bool>,
}

// ---------------------------------------------------------------------------
// Query DTOs for W2 Mimir proxy endpoints.
// ---------------------------------------------------------------------------

/// Payload for `POST /api/slos/preview`.
///
/// Given two raw PromQL strings (good / total events), the handler asks Mimir
/// for a `window_days` time series of the SLI ratio — used by the frontend
/// form before persisting an SLO.
#[derive(Debug, Clone, Deserialize)]
pub struct PreviewRequest {
    pub good_events_query: String,
    pub total_events_query: String,
    pub window_days: i32,
    #[serde(default)]
    pub step: Option<String>,
}

/// Query parameters for `GET /api/slos/:id/sli`.
#[derive(Debug, Clone, Deserialize)]
pub struct SliQuery {
    #[serde(default = "default_sli_window")]
    pub window: String,
    #[serde(default = "default_sli_step")]
    pub step: String,
}

fn default_sli_window() -> String {
    "28d".to_string()
}

fn default_sli_step() -> String {
    "5m".to_string()
}

/// Query parameters for `GET /api/slos/:id/budget/history`.
#[derive(Debug, Clone, Deserialize)]
pub struct BudgetHistoryQuery {
    pub days: Option<i32>,
}

// ---------------------------------------------------------------------------
// Response DTOs — W3 ruler sync endpoints.
// ---------------------------------------------------------------------------

/// Response payload for `POST /api/slos/:id/sync-rules` and also embedded in
/// create/update responses to surface ruler state back to the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResult {
    pub slo_id: Uuid,
    /// `true` when the rule group was pushed to Mimir successfully.
    pub synced: bool,
    /// Hash of the pushed rule group YAML. `None` when the sync was skipped
    /// (e.g. no Mimir backend configured).
    pub recording_rules_hash: Option<String>,
    /// Human-readable description of what happened — rendered in the UI
    /// toast.
    pub message: String,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sli_type_validation_accepts_all_four() {
        assert!(Slo::is_valid_sli_type(SLI_TYPE_AVAILABILITY));
        assert!(Slo::is_valid_sli_type(SLI_TYPE_LATENCY));
        assert!(Slo::is_valid_sli_type(SLI_TYPE_ERROR_RATE));
        assert!(Slo::is_valid_sli_type(SLI_TYPE_CUSTOM));
    }

    #[test]
    fn sli_type_validation_rejects_unknown_or_uppercase() {
        assert!(!Slo::is_valid_sli_type("Availability"));
        assert!(!Slo::is_valid_sli_type("throughput"));
        assert!(!Slo::is_valid_sli_type(""));
        assert!(!Slo::is_valid_sli_type("AVAILABILITY"));
    }

    #[test]
    fn window_days_validation_only_accepts_7_28_30() {
        assert!(Slo::is_valid_window_days(7));
        assert!(Slo::is_valid_window_days(28));
        assert!(Slo::is_valid_window_days(30));
        assert!(!Slo::is_valid_window_days(0));
        assert!(!Slo::is_valid_window_days(14));
        assert!(!Slo::is_valid_window_days(90));
        assert!(!Slo::is_valid_window_days(-7));
    }

    #[test]
    fn burn_severity_constants_exhaustive() {
        assert_eq!(ALL_BURN_SEVERITIES.len(), 2);
        assert!(ALL_BURN_SEVERITIES.contains(&BURN_SEVERITY_PAGE));
        assert!(ALL_BURN_SEVERITIES.contains(&BURN_SEVERITY_TICKET));
    }

    #[test]
    fn create_slo_request_serde_roundtrip_with_defaults() {
        // A minimal JSON payload; optional fields and defaulted fields must
        // fall back gracefully.
        let json = r#"{
            "name": "checkout-availability",
            "sli_type": "availability",
            "good_events_query": "sum(rate(http_requests_total{status!~\"5..\"}[5m]))",
            "total_events_query": "sum(rate(http_requests_total[5m]))",
            "objective_pct": 99.9,
            "window_days": 28
        }"#;

        let parsed: CreateSloRequest = serde_json::from_str(json).expect("deserialize");

        assert_eq!(parsed.name, "checkout-availability");
        assert_eq!(parsed.sli_type, SLI_TYPE_AVAILABILITY);
        assert_eq!(parsed.objective_pct, 99.9);
        assert_eq!(parsed.window_days, 28);
        assert_eq!(parsed.burn_rate_policy, "mwmbr_default");
        assert!(parsed.enabled);
        assert!(parsed.description.is_none());
        assert!(parsed.component_id.is_none());

        // Round-trip back to JSON and re-parse to ensure every field
        // survives serialization.
        let reserialized = serde_json::to_string(&parsed).expect("serialize");
        let reparsed: CreateSloRequest =
            serde_json::from_str(&reserialized).expect("re-deserialize");
        assert_eq!(reparsed.name, parsed.name);
        assert_eq!(reparsed.objective_pct, parsed.objective_pct);
        assert_eq!(reparsed.window_days, parsed.window_days);
        assert_eq!(reparsed.burn_rate_policy, parsed.burn_rate_policy);
    }

    #[test]
    fn update_slo_request_default_is_all_none() {
        let req = UpdateSloRequest::default();
        assert!(req.name.is_none());
        assert!(req.objective_pct.is_none());
        assert!(req.window_days.is_none());
        assert!(req.enabled.is_none());
    }
}
