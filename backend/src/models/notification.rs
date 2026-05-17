use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Notification {
    pub id: Uuid,
    pub user_id: Uuid,
    pub tenant_id: Option<Uuid>,
    pub event_type: String,
    pub title: String,
    pub description: String,
    pub payload: serde_json::Value,
    pub related_id: Option<Uuid>,
    pub is_read: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct NotificationListQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct CreateNotificationRequest {
    pub user_id: Uuid,
    pub tenant_id: Option<Uuid>,
    pub event_type: String,
    pub title: String,
    pub description: String,
    #[serde(default = "serde_json::Value::default")]
    pub payload: serde_json::Value,
    pub related_id: Option<Uuid>,
}
