use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    #[serde(skip_serializing)]
    pub password_hash: Option<String>,
    pub role: String,
    pub tenant_id: Option<Uuid>,
    pub email: Option<String>,
    pub is_active: bool,
    pub last_login_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub microsoft_id: Option<String>,
    pub cognito_sub: Option<String>,
    pub auth_method: String,
    pub invite_token: Option<Uuid>,
    pub invite_token_expires_at: Option<DateTime<Utc>>,
    pub must_change_password: bool,
}

#[derive(Debug, Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub password: String,
    pub role: String,
    pub tenant_id: Option<Uuid>,
    pub email: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateUserRequest {
    pub username: Option<String>,
    pub password: Option<String>,
    pub role: Option<String>,
    pub tenant_id: Option<Uuid>,
    pub email: Option<String>,
    pub is_active: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct InviteUserRequest {
    pub email: String,
    pub role: Option<String>,
    pub tenant_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub user: UserInfo,
    pub token: String,
}

#[derive(Debug, Serialize)]
pub struct UserInfo {
    pub id: Uuid,
    pub username: String,
    pub role: String,
    pub tenant_id: Option<Uuid>,
    pub email: Option<String>,
    pub is_active: bool,
    pub auth_method: String,
    pub must_change_password: bool,
}

impl From<User> for UserInfo {
    fn from(user: User) -> Self {
        Self {
            id: user.id,
            username: user.username,
            role: user.role,
            tenant_id: user.tenant_id,
            email: user.email,
            is_active: user.is_active,
            auth_method: user.auth_method,
            must_change_password: user.must_change_password,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct RedeemInviteRequest {
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct InviteResponse {
    pub user: UserInfo,
    pub invite_link: String,
}

#[derive(Debug, Serialize)]
pub struct ValidateInviteResponse {
    pub email: String,
    pub username: String,
}

#[derive(Debug, Deserialize)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}
