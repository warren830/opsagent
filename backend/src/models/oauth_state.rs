use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

/// Temporary OAuth state for PKCE flow (10-min TTL, one-time use)
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct OAuthState {
    pub id: Uuid,
    pub state: String,
    pub provider: String,
    pub code_verifier: String,
    pub redirect_uri: Option<String>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}
