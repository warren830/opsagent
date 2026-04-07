use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CloudAccount {
    pub id: Uuid,
    pub provider: String,
    pub name: String,
    pub account_id: Option<String>,
    pub config: serde_json::Value,
    pub secret_arn: Option<String>,
    pub tenant_id: Option<Uuid>,
    pub is_mock: bool,
    pub role_arn: Option<String>,
    pub profile: Option<String>,
    pub regions: Vec<String>,
    pub source: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateCloudAccountRequest {
    pub provider: String,
    pub name: String,
    pub account_id: Option<String>,
    #[serde(default)]
    pub config: serde_json::Value,
    pub secret_arn: Option<String>,
    pub role_arn: Option<String>,
    pub profile: Option<String>,
    pub regions: Option<Vec<String>>,
    pub source: Option<String>,
    pub tenant_id: Option<Uuid>,
    #[serde(default)]
    pub is_mock: bool,
    /// If true, trigger Organization discovery after creating this account
    #[serde(default)]
    pub discover_org: bool,
}

#[derive(Debug, Deserialize)]
pub struct UpdateCloudAccountRequest {
    pub provider: Option<String>,
    pub name: Option<String>,
    pub account_id: Option<String>,
    pub config: Option<serde_json::Value>,
    pub secret_arn: Option<String>,
    pub role_arn: Option<String>,
    pub profile: Option<String>,
    pub regions: Option<Vec<String>>,
    pub is_mock: Option<bool>,
    pub tenant_id: Option<Uuid>,
}
