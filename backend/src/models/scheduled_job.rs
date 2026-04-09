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
    pub query: Option<String>,
    pub enabled: bool,
    pub auto_jira: bool,
    pub targets: serde_json::Value,
    pub tenant_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
    pub created_by: Option<Uuid>,
    pub visibility: String,
    pub job_type: String,
    pub skill_path: Option<String>,
    pub skill_params: serde_json::Value,
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
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub auto_jira: bool,
    #[serde(default)]
    pub targets: serde_json::Value,
    #[serde(default = "default_public")]
    pub visibility: String,
    #[serde(default = "default_agent")]
    pub job_type: String,
    pub skill_path: Option<String>,
    #[serde(default)]
    pub skill_params: serde_json::Value,
}

fn default_public() -> String {
    "public".to_string()
}

fn default_agent() -> String {
    "agent".to_string()
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
    pub job_type: Option<String>,
    pub skill_path: Option<String>,
    pub skill_params: Option<serde_json::Value>,
}

fn default_utc() -> String {
    "UTC".to_string()
}

// ─── Job Runs ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct JobRun {
    pub id: Uuid,
    pub job_id: Uuid,
    pub status: String,
    pub trigger: String,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub duration_ms: Option<i64>,
    pub summary: Option<String>,
    pub output: Option<String>,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
    pub exit_code: Option<i32>,
    pub tenant_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}
