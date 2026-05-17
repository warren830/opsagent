use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Approval {
    pub id: Uuid,
    pub command: String,
    pub reason: Option<String>,
    pub requested_by: Uuid,
    pub tenant_id: Option<Uuid>,
    pub status: String,
    pub reviewed_by: Option<Uuid>,
    pub reviewed_at: Option<DateTime<Utc>>,
    pub executed_at: Option<DateTime<Utc>>,
    pub execution_result: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    /// Jira issue key (e.g. "OPS-123") — linked for webhook-based approval
    pub jira_key: Option<String>,
    /// Agent-generated execution plan (prompt + context) — used to spawn execution agent
    pub plan_detail: Option<serde_json::Value>,
    /// Who marked the execution result (success/failure) — audit trail
    pub marked_by: Option<Uuid>,
    /// When the approval was withdrawn by the requester
    pub withdrawn_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
pub struct ApprovalListQuery {
    pub status: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateApprovalRequest {
    pub command: String,
    pub reason: Option<String>,
    /// Execution plan: { "prompt": "...", "context": {...} }
    pub plan_detail: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateApprovalRequest {
    pub jira_key: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct MarkResultRequest {
    pub success: bool,
}
