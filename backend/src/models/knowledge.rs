use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct KnowledgeFile {
    pub id: Uuid,
    pub filename: String,
    pub content: String,
    pub size_bytes: i64,
    pub mime_type: Option<String>,
    pub tenant_id: Option<Uuid>,
    pub account_id: Option<Uuid>,
    pub created_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateKnowledgeRequest {
    pub filename: String,
    #[serde(default)]
    pub content: String,
    pub mime_type: Option<String>,
    pub account_id: Option<uuid::Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateKnowledgeRequest {
    pub filename: Option<String>,
    pub content: Option<String>,
    pub mime_type: Option<String>,
    pub account_id: Option<uuid::Uuid>,
}
