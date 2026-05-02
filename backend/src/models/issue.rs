use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Issue {
    pub id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub source: String,
    pub severity: String,
    pub status: String,
    pub issue_type: String,
    pub rca_result: Option<serde_json::Value>,
    pub rca_started_at: Option<DateTime<Utc>>,
    pub rca_completed_at: Option<DateTime<Utc>>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub resolved_by: Option<Uuid>,
    pub tenant_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Components this issue affects — populated from Catalog linkage
    /// (W0 migration `20260502000001_component_spec_lock.sql`).
    #[serde(default)]
    pub affected_component_ids: Vec<Uuid>,
    /// Incident this issue has been promoted into. `None` until `/promote`
    /// is called. See `20260502210001_issues_incident_ref.sql`.
    #[serde(default)]
    pub incident_id: Option<Uuid>,
}

/// Request body for `POST /api/issues/:id/promote` — turn an issue into an
/// incident. See `handlers::issue::promote_to_incident`.
#[derive(Debug, Clone, Deserialize)]
pub struct PromoteRequest {
    /// Target severity (`sev1`..`sev4`). Required — operator must decide.
    pub severity: String,
    /// Override title. Defaults to `issue.title`.
    #[serde(default)]
    pub title: Option<String>,
    /// Override impact summary. Defaults to `issue.description` if missing.
    #[serde(default)]
    pub impact_summary: Option<String>,
    /// Override affected components. Defaults to `issue.affected_component_ids`.
    #[serde(default)]
    pub affected_component_ids: Option<Vec<Uuid>>,
    /// Optional initial commander assignment.
    #[serde(default)]
    pub commander_user_id: Option<Uuid>,
    /// Extra labels merged into the incident JSONB.
    #[serde(default)]
    pub labels: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct IssueListQuery {
    pub status: Option<String>,
    pub severity: Option<String>,
    pub issue_type: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateIssueRequest {
    pub title: Option<String>,
    pub description: Option<String>,
    pub severity: Option<String>,
    pub status: Option<String>,
}
