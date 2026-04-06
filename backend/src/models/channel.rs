use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Channel {
    pub id: Uuid,
    pub platform: String,
    pub name: String,
    pub credentials: serde_json::Value,
    pub settings: serde_json::Value,
    pub enabled: bool,
    pub tenant_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateChannelRequest {
    pub platform: String,
    pub name: String,
    #[serde(default)]
    pub credentials: serde_json::Value,
    #[serde(default)]
    pub settings: serde_json::Value,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
pub struct UpdateChannelRequest {
    pub platform: Option<String>,
    pub name: Option<String>,
    pub credentials: Option<serde_json::Value>,
    pub settings: Option<serde_json::Value>,
    pub enabled: Option<bool>,
}

fn default_true() -> bool {
    true
}
