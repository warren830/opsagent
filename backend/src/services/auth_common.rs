use chrono::Utc;
use jsonwebtoken::{EncodingKey, Header, encode};
use uuid::Uuid;

use crate::config::AppConfig;
use crate::error::AppResult;
use crate::middleware::auth::Claims;
use crate::models::user::User;

/// Create a JWT access token (short-lived, configurable expiry)
pub fn create_access_token(config: &AppConfig, user: &User) -> AppResult<String> {
    let now = Utc::now().timestamp() as usize;
    let expire_secs = config.jwt_access_token_expire_minutes * 60;

    let claims = Claims {
        sub: user.id,
        role: user.role.clone(),
        tenant_id: user.tenant_id,
        username: user.username.clone(),
        token_type: "access".to_string(),
        iat: now,
        exp: now + expire_secs as usize,
    };

    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(config.jwt_secret.as_bytes()),
    )?;

    Ok(token)
}

/// Create a JWT refresh token (long-lived, used for rotation)
pub fn create_refresh_token_jwt(config: &AppConfig, user_id: Uuid) -> AppResult<String> {
    let now = Utc::now().timestamp() as usize;
    let expire_secs = config.jwt_refresh_token_expire_days * 86400;

    let claims = Claims {
        sub: user_id,
        role: String::new(),
        tenant_id: None,
        username: String::new(),
        token_type: "refresh".to_string(),
        iat: now,
        exp: now + expire_secs as usize,
    };

    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(config.jwt_secret.as_bytes()),
    )?;

    Ok(token)
}

/// Build a Set-Cookie header value for the refresh token (HttpOnly, Secure, SameSite=Lax)
pub fn refresh_token_cookie(config: &AppConfig, token: &str) -> String {
    let max_age = config.jwt_refresh_token_expire_days * 86400;
    format!(
        "refresh_token={}; HttpOnly; Secure; SameSite=Lax; Path=/api/auth; Max-Age={}",
        token, max_age
    )
}

/// Build a Set-Cookie header value to clear the refresh token cookie
pub fn clear_refresh_token_cookie(_config: &AppConfig) -> String {
    "refresh_token=; HttpOnly; Secure; SameSite=Lax; Path=/api/auth; Max-Age=0".to_string()
}

/// Extract refresh_token from Cookie header
pub fn extract_refresh_token_from_cookie(cookie_header: Option<&str>) -> Option<String> {
    let cookie_str = cookie_header?;
    for cookie in cookie_str.split(';') {
        let cookie = cookie.trim();
        if let Some(token) = cookie.strip_prefix("refresh_token=")
            && !token.is_empty()
        {
            return Some(token.to_string());
        }
    }
    None
}
