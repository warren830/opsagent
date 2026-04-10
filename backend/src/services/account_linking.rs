use sqlx::PgPool;
use uuid::Uuid;

use crate::error::AppResult;
use crate::models::user::User;

/// Find or create a user based on OAuth provider info.
/// Logic: lookup by provider_id → lookup by email (link) → create new.
pub async fn find_or_create_oauth_user(
    pool: &PgPool,
    provider: &str,    // "microsoft" or "cognito"
    provider_id: &str, // microsoft_id or cognito_sub
    email: Option<&str>,
    display_name: &str,
) -> AppResult<User> {
    // 1. Lookup by provider ID
    let provider_column = match provider {
        "microsoft" => "microsoft_id",
        "cognito" => "cognito_sub",
        _ => return Err(crate::error::AppError::OAuth(format!("Unknown provider: {}", provider))),
    };

    let query = format!("SELECT * FROM users WHERE {} = $1", provider_column);
    if let Some(user) = sqlx::query_as::<_, User>(&query)
        .bind(provider_id)
        .fetch_optional(pool)
        .await?
    {
        // Update last_login
        sqlx::query("UPDATE users SET last_login_at = NOW() WHERE id = $1")
            .bind(user.id)
            .execute(pool)
            .await?;
        return Ok(user);
    }

    // 2. Lookup by email (account linking)
    if let Some(email) = email
        && let Some(user) = sqlx::query_as::<_, User>("SELECT * FROM users WHERE email = $1")
            .bind(email)
            .fetch_optional(pool)
            .await?
    {
        // Link the provider ID to the existing account
        let update_query = format!(
            "UPDATE users SET {} = $1, last_login_at = NOW(), auth_method = CASE WHEN auth_method = 'local' THEN '{}' ELSE auth_method || ',{}' END WHERE id = $2",
            provider_column, provider, provider
        );
        sqlx::query(&update_query)
            .bind(provider_id)
            .bind(user.id)
            .execute(pool)
            .await?;

        // Re-fetch to get updated fields
        let updated = sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = $1")
            .bind(user.id)
            .fetch_one(pool)
            .await?;
        return Ok(updated);
    }

    // 3. Create new user — first user ever becomes super_admin
    let username = email.unwrap_or(display_name);
    let user_id = Uuid::new_v4();

    let user_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await?;
    let role = if user_count == 0 { "super_admin" } else { "member" };

    let insert_query = format!(
        r#"INSERT INTO users (id, username, role, email, auth_method, {})
           VALUES ($1, $2, $3, $4, $5, $6)
           RETURNING *"#,
        provider_column
    );

    let user = sqlx::query_as::<_, User>(&insert_query)
        .bind(user_id)
        .bind(username)
        .bind(role)
        .bind(email)
        .bind(provider)
        .bind(provider_id)
        .fetch_one(pool)
        .await?;

    Ok(user)
}
