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
    /// Catalog Components affected by this issue. Populated by the alerts
    /// webhook (via label/runtime matching) and surfaced on the issue
    /// detail UI so the on-call can jump to the service catalog entry.
    /// Added in migration `20260502000001_component_spec_lock.sql`.
    #[serde(default)]
    pub affected_component_ids: Vec<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
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
