use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TelemetryConfig {
    pub id: Uuid,
    pub provider: String,
    pub config: serde_json::Value,
    pub enabled: bool,
    pub tenant_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct UpsertTelemetryRequest {
    #[serde(default = "default_grafana")]
    pub provider: String,
    #[serde(default)]
    pub config: serde_json::Value,
    #[serde(default)]
    pub enabled: bool,
}

fn default_grafana() -> String {
    "grafana".to_string()
}
