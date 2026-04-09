use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct GlossaryEntry {
    pub id: Uuid,
    pub term: String,
    pub full_name: Option<String>,
    pub description: Option<String>,
    pub aliases: Vec<String>,
    pub aws_accounts: Vec<String>,
    pub services: Vec<String>,
    pub tenant_id: Option<Uuid>,
    pub account_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateGlossaryRequest {
    pub term: String,
    pub full_name: Option<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub aws_accounts: Vec<String>,
    #[serde(default)]
    pub services: Vec<String>,
    pub account_id: Option<uuid::Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateGlossaryRequest {
    pub term: Option<String>,
    pub full_name: Option<String>,
    pub description: Option<String>,
    pub aliases: Option<Vec<String>>,
    pub aws_accounts: Option<Vec<String>>,
    pub services: Option<Vec<String>>,
    pub account_id: Option<uuid::Uuid>,
}
