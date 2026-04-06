use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Resource {
    pub id: Uuid,
    pub resource_type: String,
    pub name: String,
    pub account_id: Option<String>,
    pub region: Option<String>,
    pub arn: Option<String>,
    pub status: String,
    pub tags: serde_json::Value,
    pub raw_data: serde_json::Value,
    pub tenant_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct ResourceListQuery {
    #[serde(rename = "type")]
    pub resource_type: Option<String>,
    pub region: Option<String>,
    pub q: Option<String>,
}
