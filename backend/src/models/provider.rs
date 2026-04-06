use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Provider {
    pub id: Uuid,
    pub name: String,
    pub provider_type: String,
    pub config: serde_json::Value,
    pub secret_arn: Option<String>,
    pub is_default: bool,
    pub tenant_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateProviderRequest {
    pub name: String,
    pub provider_type: String,
    pub config: serde_json::Value,
    #[serde(default)]
    pub is_default: bool,
}

#[derive(Debug, Deserialize)]
pub struct UpdateProviderRequest {
    pub name: Option<String>,
    pub provider_type: Option<String>,
    pub config: Option<serde_json::Value>,
    pub is_default: Option<bool>,
}

/// Available provider type for frontend dropdown
#[derive(Debug, Serialize)]
pub struct ProviderTypeOption {
    pub value: String,
    pub label: String,
}
