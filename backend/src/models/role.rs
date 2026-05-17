use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Role {
    pub id: Uuid,
    pub name: String,
    pub label: String,
    pub description: String,
    pub permissions: serde_json::Value,
    pub is_system: bool,
    pub tenant_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateRoleRequest {
    pub name: String,
    pub label: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_permissions")]
    pub permissions: serde_json::Value,
    #[serde(default)]
    pub is_system: bool,
}

#[derive(Debug, Deserialize)]
pub struct UpdateRoleRequest {
    pub name: Option<String>,
    pub label: Option<String>,
    pub description: Option<String>,
    pub permissions: Option<serde_json::Value>,
    pub is_system: Option<bool>,
}

fn default_permissions() -> serde_json::Value {
    serde_json::json!([])
}
