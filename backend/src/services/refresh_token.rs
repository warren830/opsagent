use hmac::{Hmac, Mac};
use jsonwebtoken::{DecodingKey, Validation, decode};
use sha2::Sha256;
use sqlx::PgPool;
use uuid::Uuid;

use crate::config::AppConfig;
use crate::error::{AppError, AppResult};
use crate::middleware::auth::Claims;
use crate::models::refresh_token::RefreshToken;
use crate::models::user::User;
use crate::services::auth_common;

type HmacSha256 = Hmac<Sha256>;

/// Hash a refresh token JWT using HMAC-SHA256 with the JWT secret
pub fn hash_refresh_token(token: &str, jwt_secret: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(jwt_secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(token.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

/// Create a new refresh token and store it in DB. Returns the JWT string.
pub async fn create_refresh_token(
    pool: &PgPool,
    config: &AppConfig,
    user_id: Uuid,
    parent_token_id: Option<Uuid>,
    family_id: Option<Uuid>,
    ip_address: Option<&str>,
    user_agent: Option<&str>,
) -> AppResult<(String, Uuid)> {
    let jwt = auth_common::create_refresh_token_jwt(config, user_id)?;
    let token_hash = hash_refresh_token(&jwt, &config.jwt_secret);
    let family = family_id.unwrap_or_else(Uuid::new_v4);
    let expire_days = config.jwt_refresh_token_expire_days as i64;

    let token = sqlx::query_as::<_, RefreshToken>(
        r#"INSERT INTO refresh_tokens (user_id, token_hash, family_id, parent_token_id, ip_address, user_agent, expires_at)
           VALUES ($1, $2, $3, $4, $5, $6, NOW() + make_interval(days => $7))
           RETURNING *"#,
    )
    .bind(user_id)
    .bind(&token_hash)
    .bind(family)
    .bind(parent_token_id)
    .bind(ip_address)
    .bind(user_agent)
    .bind(expire_days as i32)
    .fetch_one(pool)
    .await?;

    Ok((jwt, token.id))
}

/// Validate a refresh token JWT → rotate → return (new_jwt, User).
/// Implements theft detection: if the old token already has children, revoke entire family.
pub async fn validate_and_rotate(
    pool: &PgPool,
    config: &AppConfig,
    refresh_jwt: &str,
    ip_address: Option<&str>,
    user_agent: Option<&str>,
) -> AppResult<(String, String, User)> {
    // 1. Decode the refresh JWT
    let token_data = decode::<Claims>(
        refresh_jwt,
        &DecodingKey::from_secret(config.jwt_secret.as_bytes()),
        &Validation::default(),
    )
    .map_err(|e| AppError::Unauthorized(format!("Invalid refresh token: {}", e)))?;

    if token_data.claims.token_type != "refresh" {
        return Err(AppError::Unauthorized("Not a refresh token".to_string()));
    }

    let user_id = token_data.claims.sub;
    let token_hash = hash_refresh_token(refresh_jwt, &config.jwt_secret);

    // 2. Find the token in DB
    let stored =
        sqlx::query_as::<_, RefreshToken>("SELECT * FROM refresh_tokens WHERE token_hash = $1 AND user_id = $2")
            .bind(&token_hash)
            .bind(user_id)
            .fetch_optional(pool)
            .await?
            .ok_or_else(|| AppError::Unauthorized("Refresh token not found".to_string()))?;

    let (token_id, family_id, is_revoked) = (stored.id, stored.family_id, stored.is_revoked);

    // 3. If already revoked → theft detected → revoke entire family
    if is_revoked {
        revoke_family(pool, family_id, "theft").await?;
        return Err(AppError::TokenTheft);
    }

    // 4. Check if this token already has children (replay attack)
    let has_children: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM refresh_tokens WHERE parent_token_id = $1")
        .bind(token_id)
        .fetch_one(pool)
        .await?;

    if has_children.0 > 0 {
        // Replay detected → revoke entire family
        revoke_family(pool, family_id, "theft").await?;
        return Err(AppError::TokenTheft);
    }

    // 5. Revoke the current token (mark as rotated)
    revoke_token(pool, token_id, "rotation").await?;

    // 6. Fetch the user
    let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = $1 AND is_active = true")
        .bind(user_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::Unauthorized("User not found or inactive".to_string()))?;

    // 7. Create new access + refresh tokens
    let access_jwt = auth_common::create_access_token(config, &user)?;
    let (new_refresh_jwt, _) = create_refresh_token(
        pool,
        config,
        user_id,
        Some(token_id),
        Some(family_id),
        ip_address,
        user_agent,
    )
    .await?;

    Ok((access_jwt, new_refresh_jwt, user))
}

/// Revoke a single refresh token
pub async fn revoke_token(pool: &PgPool, token_id: Uuid, reason: &str) -> AppResult<()> {
    sqlx::query("UPDATE refresh_tokens SET is_revoked = true, revoked_reason = $1 WHERE id = $2")
        .bind(reason)
        .bind(token_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Revoke an entire token family (theft detection)
pub async fn revoke_family(pool: &PgPool, family_id: Uuid, reason: &str) -> AppResult<()> {
    sqlx::query(
        "UPDATE refresh_tokens SET is_revoked = true, revoked_reason = $1 WHERE family_id = $2 AND is_revoked = false",
    )
    .bind(reason)
    .bind(family_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Revoke all refresh tokens for a user
pub async fn revoke_all_user_tokens(pool: &PgPool, user_id: Uuid) -> AppResult<u64> {
    let result = sqlx::query(
        "UPDATE refresh_tokens SET is_revoked = true, revoked_reason = 'revoke_all' WHERE user_id = $1 AND is_revoked = false",
    )
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// Cleanup expired and old revoked refresh tokens
pub async fn cleanup_expired(pool: &PgPool) -> AppResult<u64> {
    let result = sqlx::query(
        "DELETE FROM refresh_tokens WHERE expires_at < NOW() OR (is_revoked = true AND created_at < NOW() - INTERVAL '30 days')",
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// Revoke a refresh token by its hash (used during logout)
pub async fn revoke_by_hash(pool: &PgPool, token_hash: &str, reason: &str) -> AppResult<()> {
    sqlx::query("UPDATE refresh_tokens SET is_revoked = true, revoked_reason = $1 WHERE token_hash = $2")
        .bind(reason)
        .bind(token_hash)
        .execute(pool)
        .await?;
    Ok(())
}
