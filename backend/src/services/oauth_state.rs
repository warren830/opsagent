use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::RngCore;
use sha2::{Digest, Sha256};
use sqlx::PgPool;

use crate::error::{AppError, AppResult};
use crate::models::oauth_state::OAuthState;

/// Generate PKCE code_verifier + code_challenge (S256)
pub fn generate_pkce() -> (String, String) {
    let mut verifier_bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut verifier_bytes);
    let code_verifier = URL_SAFE_NO_PAD.encode(verifier_bytes);

    let mut hasher = Sha256::new();
    hasher.update(code_verifier.as_bytes());
    let code_challenge = URL_SAFE_NO_PAD.encode(hasher.finalize());

    (code_verifier, code_challenge)
}

/// Generate a random state string
fn generate_state() -> String {
    let mut state_bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut state_bytes);
    URL_SAFE_NO_PAD.encode(state_bytes)
}

/// Create an OAuth state entry in DB (10-min TTL). Returns (state, code_challenge).
pub async fn create_oauth_state(
    pool: &PgPool,
    provider: &str,
    redirect_uri: Option<&str>,
) -> AppResult<(String, String)> {
    let state = generate_state();
    let (code_verifier, code_challenge) = generate_pkce();

    sqlx::query(
        r#"INSERT INTO oauth_states (state, provider, code_verifier, redirect_uri)
           VALUES ($1, $2, $3, $4)"#,
    )
    .bind(&state)
    .bind(provider)
    .bind(&code_verifier)
    .bind(redirect_uri)
    .execute(pool)
    .await?;

    Ok((state, code_challenge))
}

/// Validate and consume an OAuth state (one-time use). Returns the code_verifier for PKCE.
pub async fn validate_and_consume_state(
    pool: &PgPool,
    state: &str,
    provider: &str,
) -> AppResult<(String, Option<String>)> {
    let oauth_state = sqlx::query_as::<_, OAuthState>(
        r#"DELETE FROM oauth_states
           WHERE state = $1 AND provider = $2 AND expires_at > NOW()
           RETURNING *"#,
    )
    .bind(state)
    .bind(provider)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::OAuth("Invalid or expired OAuth state".to_string()))?;

    Ok((oauth_state.code_verifier, oauth_state.redirect_uri))
}

/// Cleanup expired OAuth states (called periodically)
pub async fn cleanup_expired_states(pool: &PgPool) -> AppResult<u64> {
    let result = sqlx::query("DELETE FROM oauth_states WHERE expires_at < NOW()")
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pkce_generation() {
        let (verifier, challenge) = generate_pkce();
        assert!(!verifier.is_empty());
        assert!(!challenge.is_empty());
        assert_ne!(verifier, challenge);

        // Verify S256: challenge = base64url(sha256(verifier))
        let mut hasher = Sha256::new();
        hasher.update(verifier.as_bytes());
        let expected = URL_SAFE_NO_PAD.encode(hasher.finalize());
        assert_eq!(challenge, expected);
    }

    #[test]
    fn test_state_generation() {
        let s1 = generate_state();
        let s2 = generate_state();
        assert_ne!(s1, s2);
        assert!(s1.len() >= 32);
    }
}
