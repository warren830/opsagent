use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Cluster {
    pub id: Uuid,
    pub name: String,
    pub cloud: String,
    pub cluster_type: String,
    pub account_id: Option<String>,
    pub region: Option<String>,
    pub role_name: Option<String>,
    pub description: Option<String>,
    pub is_discovered: bool,
    pub status: String,
    pub last_seen_at: Option<DateTime<Utc>>,
    pub config: serde_json::Value,
    pub tenant_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateClusterRequest {
    pub name: String,
    #[serde(default = "default_aws")]
    pub cloud: String,
    #[serde(default = "default_eks")]
    pub cluster_type: String,
    pub account_id: Option<String>,
    pub region: Option<String>,
    pub role_name: Option<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub config: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct UpdateClusterRequest {
    pub name: Option<String>,
    pub cloud: Option<String>,
    pub cluster_type: Option<String>,
    pub account_id: Option<String>,
    pub region: Option<String>,
    pub role_name: Option<String>,
    pub description: Option<String>,
    pub status: Option<String>,
    pub config: Option<serde_json::Value>,
}

fn default_aws() -> String {
    "aws".to_string()
}

fn default_eks() -> String {
    "eks".to_string()
}
