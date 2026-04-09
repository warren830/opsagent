use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct DeploymentEvent {
    pub id: Uuid,
    pub cluster_id: Uuid,
    pub namespace: String,
    pub rollout_name: String,
    pub action: String,
    pub detail: serde_json::Value,
    pub user_id: Option<Uuid>,
    pub tenant_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}
