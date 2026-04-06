use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ScheduledJob {
    pub id: Uuid,
    pub name: String,
    pub cron_expression: String,
    pub timezone: String,
    pub query: String,
    pub enabled: bool,
    pub auto_jira: bool,
    pub targets: serde_json::Value,
    pub tenant_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
    pub created_by: Option<Uuid>,
    pub visibility: String,
    pub last_run_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateScheduledJobRequest {
    pub name: String,
    pub cron_expression: String,
    #[serde(default = "default_utc")]
    pub timezone: String,
    pub query: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub auto_jira: bool,
    #[serde(default)]
    pub targets: serde_json::Value,
    #[serde(default = "default_public")]
    pub visibility: String,
}

fn default_public() -> String {
    "public".to_string()
}

#[derive(Debug, Deserialize)]
pub struct UpdateScheduledJobRequest {
    pub name: Option<String>,
    pub cron_expression: Option<String>,
    pub timezone: Option<String>,
    pub query: Option<String>,
    pub enabled: Option<bool>,
    pub auto_jira: Option<bool>,
    pub targets: Option<serde_json::Value>,
}

fn default_utc() -> String {
    "UTC".to_string()
}
