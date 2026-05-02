//! Incident Command Center types — W1 core schema + state machine.
//!
//! See:
//! - `docs/platform-evolution.md` §5 (data model §5.2, state machine §5.3).
//! - `backend/src/migrations/20260502200001_incidents_core.sql`.
//!
//! Status and severity are stored as plain VARCHAR in Postgres (mirrors
//! `issues.status` / `issues.severity`), so Rust uses `String` and the
//! constants below define the allowed value set. Helper functions like
//! `Incident::is_valid_status` and the `state_machine` module enforce the
//! actual transition rules.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Status constants (mirror the CHECK constraint in the migration).
// ---------------------------------------------------------------------------
pub const STATUS_TRIGGERED: &str = "triggered";
pub const STATUS_ACKNOWLEDGED: &str = "acknowledged";
pub const STATUS_INVESTIGATING: &str = "investigating";
pub const STATUS_IDENTIFIED: &str = "identified";
pub const STATUS_MITIGATED: &str = "mitigated";
pub const STATUS_RESOLVED: &str = "resolved";
pub const STATUS_POSTMORTEM_DRAFT: &str = "postmortem_draft";
pub const STATUS_POSTMORTEM_PUBLISHED: &str = "postmortem_published";
pub const STATUS_CLOSED: &str = "closed";

pub const ALL_STATUSES: &[&str] = &[
    STATUS_TRIGGERED,
    STATUS_ACKNOWLEDGED,
    STATUS_INVESTIGATING,
    STATUS_IDENTIFIED,
    STATUS_MITIGATED,
    STATUS_RESOLVED,
    STATUS_POSTMORTEM_DRAFT,
    STATUS_POSTMORTEM_PUBLISHED,
    STATUS_CLOSED,
];

// ---------------------------------------------------------------------------
// Severity constants.
// ---------------------------------------------------------------------------
pub const SEVERITY_SEV1: &str = "sev1";
pub const SEVERITY_SEV2: &str = "sev2";
pub const SEVERITY_SEV3: &str = "sev3";
pub const SEVERITY_SEV4: &str = "sev4";

pub const ALL_SEVERITIES: &[&str] = &[
    SEVERITY_SEV1,
    SEVERITY_SEV2,
    SEVERITY_SEV3,
    SEVERITY_SEV4,
];

// ---------------------------------------------------------------------------
// Detection source.
// ---------------------------------------------------------------------------
pub const DETECTION_SOURCE_ALERT: &str = "alert";
pub const DETECTION_SOURCE_MANUAL: &str = "manual";
pub const DETECTION_SOURCE_SLO_BURN: &str = "slo_burn";
pub const DETECTION_SOURCE_CHAOS: &str = "chaos";
pub const DETECTION_SOURCE_SYNTHETIC: &str = "synthetic";

pub const ALL_DETECTION_SOURCES: &[&str] = &[
    DETECTION_SOURCE_ALERT,
    DETECTION_SOURCE_MANUAL,
    DETECTION_SOURCE_SLO_BURN,
    DETECTION_SOURCE_CHAOS,
    DETECTION_SOURCE_SYNTHETIC,
];

// ---------------------------------------------------------------------------
// Participant role.
// ---------------------------------------------------------------------------
pub const PARTICIPANT_ROLE_COMMANDER: &str = "commander";
pub const PARTICIPANT_ROLE_SCRIBE: &str = "scribe";
pub const PARTICIPANT_ROLE_RESPONDER: &str = "responder";
pub const PARTICIPANT_ROLE_OBSERVER: &str = "observer";
pub const PARTICIPANT_ROLE_STAKEHOLDER: &str = "stakeholder";

pub const ALL_PARTICIPANT_ROLES: &[&str] = &[
    PARTICIPANT_ROLE_COMMANDER,
    PARTICIPANT_ROLE_SCRIBE,
    PARTICIPANT_ROLE_RESPONDER,
    PARTICIPANT_ROLE_OBSERVER,
    PARTICIPANT_ROLE_STAKEHOLDER,
];

// ---------------------------------------------------------------------------
// Update audience.
// ---------------------------------------------------------------------------
pub const UPDATE_AUDIENCE_INTERNAL: &str = "internal";
pub const UPDATE_AUDIENCE_CUSTOMERS: &str = "customers";
pub const UPDATE_AUDIENCE_STAKEHOLDERS: &str = "stakeholders";
pub const UPDATE_AUDIENCE_STATUS_PAGE: &str = "status_page";

pub const ALL_UPDATE_AUDIENCES: &[&str] = &[
    UPDATE_AUDIENCE_INTERNAL,
    UPDATE_AUDIENCE_CUSTOMERS,
    UPDATE_AUDIENCE_STAKEHOLDERS,
    UPDATE_AUDIENCE_STATUS_PAGE,
];

// ---------------------------------------------------------------------------
// Core record.
// ---------------------------------------------------------------------------

/// An incident — a high-priority, lifecycle-tracked event that rolls up
/// alerts, issues, agent activity, and deployments into a single war-room
/// record. Mirrors the `incidents` table.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Incident {
    pub id: Uuid,
    pub tenant_id: Option<Uuid>,
    pub number: i64,
    pub title: String,
    pub severity: String,
    pub status: String,
    pub commander_user_id: Option<Uuid>,
    pub scribe_user_id: Option<Uuid>,
    pub impact_summary: Option<String>,
    pub affected_component_ids: Vec<Uuid>,
    pub affected_customer_tier: Option<String>,
    pub detection_source: String,
    pub source_issue_id: Option<Uuid>,
    pub started_at: DateTime<Utc>,
    pub detected_at: DateTime<Utc>,
    pub acknowledged_at: Option<DateTime<Utc>>,
    pub mitigated_at: Option<DateTime<Utc>>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub closed_at: Option<DateTime<Utc>>,
    pub war_room_channel_ref: Option<serde_json::Value>,
    pub bridge_url: Option<String>,
    pub jira_key: Option<String>,
    pub postmortem_doc_ref: Option<serde_json::Value>,
    pub root_cause: Option<String>,
    pub root_cause_category: Option<String>,
    pub labels: serde_json::Value,
    pub slo_budget_burn: Option<serde_json::Value>,
    pub merged_into_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Incident {
    /// Returns `true` if the given string is one of the allowed `status`
    /// values (see `ALL_STATUSES`).
    pub fn is_valid_status(status: &str) -> bool {
        ALL_STATUSES.contains(&status)
    }

    /// Returns `true` if the given string is one of the allowed `severity`
    /// values (see `ALL_SEVERITIES`).
    pub fn is_valid_severity(severity: &str) -> bool {
        ALL_SEVERITIES.contains(&severity)
    }

    /// Returns `true` if the status is a terminal state with no outgoing
    /// transitions. Only `closed` is terminal in the current state machine.
    pub fn is_terminal_status(status: &str) -> bool {
        status == STATUS_CLOSED
    }

    /// Returns `true` if the status represents an active (non-closed)
    /// incident. Useful for list queries and index filters.
    pub fn is_active(status: &str) -> bool {
        !Self::is_terminal_status(status)
    }
}

// ---------------------------------------------------------------------------
// Timeline events, participants, severity history, updates.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct IncidentTimelineEvent {
    pub id: Uuid,
    pub incident_id: Uuid,
    pub kind: String,
    pub actor: serde_json::Value,
    pub occurred_at: DateTime<Utc>,
    pub service_id: Option<Uuid>,
    pub summary: String,
    pub payload: serde_json::Value,
    pub correlation_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct IncidentParticipant {
    pub incident_id: Uuid,
    pub user_id: Uuid,
    pub role: String,
    pub joined_at: DateTime<Utc>,
    pub left_at: Option<DateTime<Utc>>,
    pub added_via: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct IncidentSeverityHistory {
    pub id: Uuid,
    pub incident_id: Uuid,
    pub from_severity: Option<String>,
    pub to_severity: String,
    pub changed_by: Option<Uuid>,
    pub reason: Option<String>,
    pub changed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct IncidentUpdate {
    pub id: Uuid,
    pub incident_id: Uuid,
    pub author_user_id: Option<Uuid>,
    pub audience: String,
    pub status_at_time: String,
    pub body_markdown: String,
    pub published_at: Option<DateTime<Utc>>,
    pub pushed_to: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Request DTOs.
// ---------------------------------------------------------------------------

/// Fields required to create a new incident. `number`, timestamps, and the
/// generated `id` are populated by the service layer / database defaults.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateIncidentRequest {
    pub title: String,
    pub severity: String,
    #[serde(default)]
    pub status: Option<String>,
    pub detection_source: String,
    #[serde(default)]
    pub impact_summary: Option<String>,
    #[serde(default)]
    pub affected_component_ids: Vec<Uuid>,
    #[serde(default)]
    pub affected_customer_tier: Option<String>,
    #[serde(default)]
    pub source_issue_id: Option<Uuid>,
    pub started_at: DateTime<Utc>,
    #[serde(default)]
    pub commander_user_id: Option<Uuid>,
    #[serde(default)]
    pub scribe_user_id: Option<Uuid>,
    #[serde(default)]
    pub bridge_url: Option<String>,
    #[serde(default)]
    pub labels: Option<serde_json::Value>,
    #[serde(default)]
    pub tenant_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransitionRequest {
    pub to_status: String,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeverityChangeRequest {
    pub to_severity: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddParticipantRequest {
    pub user_id: Uuid,
    pub role: String,
    #[serde(default)]
    pub added_via: Option<String>,
}

/// Query params for `GET /api/incidents`. All fields optional; `active_only`
/// filters out closed incidents when true.
#[derive(Debug, Clone, Deserialize)]
pub struct ListIncidentsQuery {
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub severity: Option<String>,
    #[serde(default)]
    pub component_id: Option<Uuid>,
    #[serde(default)]
    pub active_only: bool,
}

/// Editable metadata fields for `PATCH /api/incidents/:id`. Status, severity
/// and `number` are NOT here — they have dedicated endpoints (transition,
/// change_severity) and `number` is immutable.
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateIncidentRequest {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub impact_summary: Option<String>,
    #[serde(default)]
    pub affected_component_ids: Option<Vec<Uuid>>,
    #[serde(default)]
    pub affected_customer_tier: Option<String>,
    #[serde(default)]
    pub commander_user_id: Option<Uuid>,
    #[serde(default)]
    pub scribe_user_id: Option<Uuid>,
    #[serde(default)]
    pub labels: Option<serde_json::Value>,
    #[serde(default)]
    pub root_cause: Option<String>,
    #[serde(default)]
    pub root_cause_category: Option<String>,
}

/// Query params for `GET /api/incidents/:id/timeline`.
#[derive(Debug, Clone, Deserialize)]
pub struct TimelineQuery {
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub after: Option<DateTime<Utc>>,
    #[serde(default)]
    pub kind: Option<String>,
}

/// Response for `GET /api/incidents/:id` — the incident plus the last 20
/// timeline events, all participants, and the last 5 stakeholder updates.
#[derive(Debug, Clone, Serialize)]
pub struct IncidentDetail {
    #[serde(flatten)]
    pub incident: Incident,
    pub timeline: Vec<IncidentTimelineEvent>,
    pub participants: Vec<IncidentParticipant>,
    pub recent_updates: Vec<IncidentUpdate>,
}

// ---------------------------------------------------------------------------
// Tests.
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn status_validation_accepts_known_values_and_rejects_unknown() {
        for s in ALL_STATUSES {
            assert!(Incident::is_valid_status(s), "expected {s} to be valid");
        }
        assert!(!Incident::is_valid_status("open"));
        assert!(!Incident::is_valid_status(""));
        assert!(!Incident::is_valid_status("Triggered")); // case-sensitive
    }

    #[test]
    fn severity_validation_accepts_known_values_and_rejects_unknown() {
        for s in ALL_SEVERITIES {
            assert!(Incident::is_valid_severity(s), "expected {s} to be valid");
        }
        assert!(!Incident::is_valid_severity("sev0"));
        assert!(!Incident::is_valid_severity("SEV1"));
        assert!(!Incident::is_valid_severity("critical"));
    }

    #[test]
    fn terminal_and_active_status_agree() {
        // Only `closed` is terminal.
        assert!(Incident::is_terminal_status(STATUS_CLOSED));
        assert!(!Incident::is_active(STATUS_CLOSED));

        // Every other status is active.
        for s in ALL_STATUSES.iter().filter(|s| **s != STATUS_CLOSED) {
            assert!(!Incident::is_terminal_status(s), "{s} must not be terminal");
            assert!(Incident::is_active(s), "{s} must be active");
        }

        // Even an unknown status counts as active (not closed) — the state
        // machine gates the actual transitions separately.
        assert!(Incident::is_active("some_unknown_status"));
    }

    #[test]
    fn create_incident_request_serde_roundtrip() {
        let req = CreateIncidentRequest {
            title: "Checkout p99 spike".to_string(),
            severity: SEVERITY_SEV2.to_string(),
            status: Some(STATUS_TRIGGERED.to_string()),
            detection_source: DETECTION_SOURCE_ALERT.to_string(),
            impact_summary: Some("40% 5xx on order-api".to_string()),
            affected_component_ids: vec![Uuid::new_v4(), Uuid::new_v4()],
            affected_customer_tier: Some("tier0".to_string()),
            source_issue_id: Some(Uuid::new_v4()),
            started_at: Utc.with_ymd_and_hms(2026, 5, 2, 12, 4, 0).unwrap(),
            commander_user_id: Some(Uuid::new_v4()),
            scribe_user_id: None,
            bridge_url: Some("https://meet.example.com/inc-0042".to_string()),
            labels: Some(serde_json::json!({"region": "us-west-2"})),
            tenant_id: Some(Uuid::new_v4()),
        };
        let json = serde_json::to_string(&req).expect("serialize");
        let parsed: CreateIncidentRequest =
            serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.title, req.title);
        assert_eq!(parsed.severity, req.severity);
        assert_eq!(parsed.affected_component_ids, req.affected_component_ids);
        assert_eq!(parsed.started_at, req.started_at);
    }

    #[test]
    fn incident_full_serde_roundtrip() {
        let inc = Incident {
            id: Uuid::new_v4(),
            tenant_id: Some(Uuid::new_v4()),
            number: 42,
            title: "t".to_string(),
            severity: SEVERITY_SEV2.to_string(),
            status: STATUS_INVESTIGATING.to_string(),
            commander_user_id: Some(Uuid::new_v4()),
            scribe_user_id: None,
            impact_summary: Some("impact".to_string()),
            affected_component_ids: vec![Uuid::new_v4()],
            affected_customer_tier: Some("tier0".to_string()),
            detection_source: DETECTION_SOURCE_ALERT.to_string(),
            source_issue_id: Some(Uuid::new_v4()),
            started_at: Utc.with_ymd_and_hms(2026, 5, 2, 12, 4, 0).unwrap(),
            detected_at: Utc.with_ymd_and_hms(2026, 5, 2, 12, 4, 5).unwrap(),
            acknowledged_at: None,
            mitigated_at: None,
            resolved_at: None,
            closed_at: None,
            war_room_channel_ref: Some(serde_json::json!({"slack_channel_id": "C123"})),
            bridge_url: None,
            jira_key: Some("OPS-1234".to_string()),
            postmortem_doc_ref: None,
            root_cause: None,
            root_cause_category: None,
            labels: serde_json::json!({"env": "prod"}),
            slo_budget_burn: Some(serde_json::json!({"order-api.availability": 0.12})),
            merged_into_id: None,
            created_at: Utc.with_ymd_and_hms(2026, 5, 2, 12, 4, 5).unwrap(),
            updated_at: Utc.with_ymd_and_hms(2026, 5, 2, 12, 4, 5).unwrap(),
        };
        let json = serde_json::to_string(&inc).expect("serialize");
        let parsed: Incident = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.id, inc.id);
        assert_eq!(parsed.number, 42);
        assert_eq!(parsed.status, STATUS_INVESTIGATING);
        assert_eq!(parsed.affected_component_ids, inc.affected_component_ids);
        assert_eq!(parsed.labels, inc.labels);
        assert_eq!(parsed.slo_budget_burn, inc.slo_budget_burn);
    }
}
