use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct McpServer {
    pub id: Uuid,
    pub name: String,
    pub command: String,
    pub args: serde_json::Value,
    pub env: serde_json::Value,
    pub enabled: bool,
    pub tenant_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
    pub created_by: Option<Uuid>,
    pub visibility: String,
    pub transport_type: String,
    pub url: Option<String>,
    pub headers: serde_json::Value,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateMcpServerRequest {
    pub name: String,
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub args: serde_json::Value,
    #[serde(default)]
    pub env: serde_json::Value,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_public")]
    pub visibility: String,
    #[serde(default = "default_stdio")]
    pub transport_type: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub headers: serde_json::Value,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateMcpServerRequest {
    pub name: Option<String>,
    pub command: Option<String>,
    pub args: Option<serde_json::Value>,
    pub env: Option<serde_json::Value>,
    pub enabled: Option<bool>,
    pub transport_type: Option<String>,
    pub url: Option<String>,
    pub headers: Option<serde_json::Value>,
    pub description: Option<String>,
}

fn default_true() -> bool {
    true
}

fn default_public() -> String {
    "public".to_string()
}

fn default_stdio() -> String {
    "stdio".to_string()
}
