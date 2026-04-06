use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct UserAccountAccess {
    pub id: Uuid,
    pub user_id: Uuid,
    pub account_id: Uuid,
    pub role: String, // "admin" | "readonly"
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct GrantAccessRequest {
    pub user_id: Uuid,
    pub account_id: Uuid,
    #[serde(default = "default_readonly")]
    pub role: String,
}

fn default_readonly() -> String {
    "readonly".to_string()
}

/// View returned when listing users with access to an account
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct UserAccessView {
    pub user_id: Uuid,
    pub username: String,
    pub email: Option<String>,
    pub role: String,
    pub created_at: DateTime<Utc>,
}

/// Minimal account info for dropdown selectors
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AccessibleAccount {
    pub id: Uuid,
    pub provider: String,
    pub name: String,
    pub account_id: Option<String>,
    #[sqlx(default)]
    #[serde(default)]
    pub writable: bool,
}
