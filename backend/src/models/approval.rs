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
}

#[derive(Debug, Deserialize)]
pub struct ApprovalListQuery {
    pub status: Option<String>,
}
