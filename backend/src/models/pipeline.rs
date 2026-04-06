use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct PipelineRepo {
    pub id: Uuid,
    pub repo_id: String,
    pub name: String,
    pub repository: String,
    pub token_secret_arn: Option<String>,
    pub description: Option<String>,
    pub enabled: bool,
    pub tenant_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreatePipelineRepoRequest {
    pub repo_id: String,
    pub name: String,
    pub repository: String,
    pub token_secret_arn: Option<String>,
    pub description: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
pub struct UpdatePipelineRepoRequest {
    pub name: Option<String>,
    pub repository: Option<String>,
    pub token_secret_arn: Option<String>,
    pub description: Option<String>,
    pub enabled: Option<bool>,
}

fn default_true() -> bool {
    true
}
