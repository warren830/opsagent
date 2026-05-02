//! Global change-event stream model (W10 Joint Integration).
//!
//! `change_events` is the long-lived, non-incident-scoped audit of every
//! interesting mutation on a service (deploy, rollback, config change, SLO
//! burn, catalog import, manual runbook action). It complements the
//! incident-scoped `incident_timeline_events` stream — see
//! `docs/platform-evolution.md` §6.1 decision #5.
//!
//! The `incident_timeline_events` table still owns per-incident "what
//! happened during the war room" rendering; `change_events` is what the
//! agent queries when answering "what recent changes could explain this
//! burn?" regardless of whether an incident was open at the time.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Kind constants (mirror free-form VARCHAR — not CHECK-constrained in SQL so
// future kinds don't need a migration, but callers should prefer a constant
// to keep the filter set stable).
// ---------------------------------------------------------------------------
pub const KIND_DEPLOY: &str = "deploy";
pub const KIND_ROLLBACK: &str = "rollback";
pub const KIND_CONFIG: &str = "config";
pub const KIND_FEATURE_FLAG: &str = "feature_flag";
pub const KIND_SLO_BURN: &str = "slo_burn";
pub const KIND_MANUAL: &str = "manual";
pub const KIND_CATALOG_IMPORT: &str = "catalog_import";

/// Canonical kind values the handler validates against when filtering.
pub const ALL_KINDS: &[&str] = &[
    KIND_DEPLOY,
    KIND_ROLLBACK,
    KIND_CONFIG,
    KIND_FEATURE_FLAG,
    KIND_SLO_BURN,
    KIND_MANUAL,
    KIND_CATALOG_IMPORT,
];

// ---------------------------------------------------------------------------
// Source constants — where the event came from (webhook, api, import…).
// ---------------------------------------------------------------------------
pub const SOURCE_ARGOCD: &str = "argocd";
pub const SOURCE_ROLLOUT_API: &str = "rollout_api";
pub const SOURCE_SLO_BURN: &str = "slo_burn";
pub const SOURCE_MANUAL: &str = "manual";
pub const SOURCE_IMPORT_RUN: &str = "import_run";

/// One row from `change_events`.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ChangeEvent {
    pub id: Uuid,
    pub tenant_id: Option<Uuid>,
    pub kind: String,
    pub service_id: Option<Uuid>,
    pub actor: serde_json::Value,
    pub occurred_at: DateTime<Utc>,
    pub summary: String,
    pub payload: serde_json::Value,
    pub correlation_id: Option<String>,
    pub source: String,
    pub created_at: DateTime<Utc>,
}

/// Query parameters for `GET /api/change-events`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct QueryChangesParams {
    #[serde(default)]
    pub service_id: Option<Uuid>,
    #[serde(default)]
    pub since: Option<DateTime<Utc>>,
    #[serde(default)]
    pub until: Option<DateTime<Utc>>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub limit: Option<i64>,
}

impl ChangeEvent {
    /// Returns `true` if the given string is one of the canonical
    /// `ALL_KINDS` values. The column is free-form, so unknown kinds do
    /// persist — this helper is for the list endpoint's filter validation.
    pub fn is_known_kind(kind: &str) -> bool {
        ALL_KINDS.contains(&kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn known_kinds_are_complete() {
        assert!(ChangeEvent::is_known_kind(KIND_DEPLOY));
        assert!(ChangeEvent::is_known_kind(KIND_ROLLBACK));
        assert!(ChangeEvent::is_known_kind(KIND_CONFIG));
        assert!(ChangeEvent::is_known_kind(KIND_FEATURE_FLAG));
        assert!(ChangeEvent::is_known_kind(KIND_SLO_BURN));
        assert!(ChangeEvent::is_known_kind(KIND_MANUAL));
        assert!(ChangeEvent::is_known_kind(KIND_CATALOG_IMPORT));
        assert!(!ChangeEvent::is_known_kind("fake_kind"));
    }

    #[test]
    fn change_event_serde_round_trip() {
        let id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let svc_id = Uuid::new_v4();
        let event = ChangeEvent {
            id,
            tenant_id: Some(tenant_id),
            kind: KIND_DEPLOY.to_string(),
            service_id: Some(svc_id),
            actor: serde_json::json!({"type": "system", "display_name": "argocd"}),
            occurred_at: Utc.with_ymd_and_hms(2026, 5, 1, 12, 0, 0).unwrap(),
            summary: "Deploy ops-backend to prod".to_string(),
            payload: serde_json::json!({"revision": "abc123"}),
            correlation_id: Some("corr-1".to_string()),
            source: SOURCE_ARGOCD.to_string(),
            created_at: Utc.with_ymd_and_hms(2026, 5, 1, 12, 0, 1).unwrap(),
        };

        let s = serde_json::to_string(&event).expect("serialize");
        let round: ChangeEvent = serde_json::from_str(&s).expect("deserialize");

        assert_eq!(round.id, id);
        assert_eq!(round.kind, KIND_DEPLOY);
        assert_eq!(round.source, SOURCE_ARGOCD);
        assert_eq!(round.service_id, Some(svc_id));
    }

    #[test]
    fn query_params_defaults_are_empty() {
        // Via serde_json (use `{}` for empty). Avoids direct serde_urlencoded
        // dependency — axum's Query uses it internally but we don't want to
        // pin the crate version in our unit tests.
        let p: QueryChangesParams =
            serde_json::from_str("{}").expect("parse empty");
        assert!(p.service_id.is_none());
        assert!(p.since.is_none());
        assert!(p.until.is_none());
        assert!(p.kind.is_none());
        assert!(p.limit.is_none());
    }

    #[test]
    fn query_params_parses_filters() {
        let svc = Uuid::new_v4();
        let json = serde_json::json!({
            "service_id": svc,
            "kind": "deploy",
            "limit": 50
        });
        let p: QueryChangesParams =
            serde_json::from_value(json).expect("parse");
        assert_eq!(p.service_id, Some(svc));
        assert_eq!(p.kind.as_deref(), Some("deploy"));
        assert_eq!(p.limit, Some(50));
    }
}
