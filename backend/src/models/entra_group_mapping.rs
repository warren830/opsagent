use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct EntraGroupMapping {
    pub id: Uuid,
    pub group_id: String,
    pub group_name: String,
    pub role: String,
    pub tenant_id: Option<Uuid>,
    pub account_access: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateEntraGroupMappingRequest {
    pub group_id: String,
    pub group_name: Option<String>,
    pub role: Option<String>,
    pub tenant_id: Option<Uuid>,
    pub account_access: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateEntraGroupMappingRequest {
    pub group_id: Option<String>,
    pub group_name: Option<String>,
    pub role: Option<String>,
    pub tenant_id: Option<Uuid>,
    pub account_access: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountAccessEntry {
    pub account_id: Uuid,
    pub role: String,
}
