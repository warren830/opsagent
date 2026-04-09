use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TelemetryConfig {
    pub id: Uuid,
    pub name: String,
    pub provider: String,
    pub config: serde_json::Value,
    pub routing: serde_json::Value,
    pub enabled: bool,
    pub tenant_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateTelemetryRequest {
    pub name: String,
    #[serde(default = "default_grafana")]
    pub provider: String,
    #[serde(default)]
    pub config: serde_json::Value,
    #[serde(default = "default_routing")]
    pub routing: serde_json::Value,
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
pub struct UpdateTelemetryRequest {
    pub name: Option<String>,
    pub provider: Option<String>,
    pub config: Option<serde_json::Value>,
    pub routing: Option<serde_json::Value>,
    pub enabled: Option<bool>,
}

fn default_grafana() -> String {
    "grafana".to_string()
}

fn default_routing() -> serde_json::Value {
    serde_json::json!({"signals": ["metrics", "logs", "traces"], "scope": "all"})
}
