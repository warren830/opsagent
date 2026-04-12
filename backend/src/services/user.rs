use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::middleware::auth::AuthUser;
use crate::models::user::{CreateUserRequest, InviteUserRequest, UpdateUserRequest, User, UserInfo};
use crate::services::common::{require_super_admin, tenant_filter};

/// List users visible to the authenticated user.
/// Super admins see all; members see only their tenant.
pub async fn list(pool: &PgPool, auth_user: &AuthUser) -> AppResult<Vec<UserInfo>> {
    let users = match tenant_filter(auth_user) {
        None => {
            sqlx::query_as::<_, User>("SELECT * FROM users ORDER BY username")
                .fetch_all(pool)
                .await?
        }
        Some(tid) => {
            sqlx::query_as::<_, User>(
                "SELECT * FROM users WHERE tenant_id = $1 ORDER BY username",
            )
            .bind(tid)
            .fetch_all(pool)
            .await?
        }
    };

    Ok(users.into_iter().map(UserInfo::from).collect())
}

/// Create a new user (super_admin only).
pub async fn create(
    pool: &PgPool,
    auth_user: &AuthUser,
    req: CreateUserRequest,
) -> AppResult<UserInfo> {
    require_super_admin(auth_user, "create users")?;

    if req.username.trim().is_empty() || req.password.len() < 8 {
        return Err(AppError::BadRequest(
            "Username required, password must be at least 8 characters".to_string(),
        ));
    }

    if req.role != "super_admin" && req.role != "member" {
        return Err(AppError::BadRequest(
            "Role must be 'super_admin' or 'member'".to_string(),
        ));
    }

    if req.role == "member" && req.tenant_id.is_none() {
        return Err(AppError::BadRequest(
            "tenant_id is required for tenant_admin role".to_string(),
        ));
    }

    let pw = req.password.clone();
    let password_hash = tokio::task::spawn_blocking(move || bcrypt::hash(pw, 10))
        .await
        .map_err(|_| AppError::Internal("Password hashing failed".to_string()))??;

    let user = sqlx::query_as::<_, User>(
        r#"INSERT INTO users (username, password_hash, role, tenant_id, email)
           VALUES ($1, $2, $3, $4, $5)
           RETURNING *"#,
    )
    .bind(&req.username)
    .bind(&password_hash)
    .bind(&req.role)
    .bind(req.tenant_id)
    .bind(&req.email)
    .fetch_one(pool)
    .await
    .map_err(|e| {
        if let sqlx::Error::Database(ref db_err) = e
            && db_err.constraint() == Some("users_username_key")
        {
            return AppError::Conflict("Username already exists".to_string());
        }
        AppError::Database(e)
    })?;

    Ok(UserInfo::from(user))
}

/// Update an existing user (super_admin only).
pub async fn update(
    pool: &PgPool,
    auth_user: &AuthUser,
    id: Uuid,
    req: UpdateUserRequest,
) -> AppResult<UserInfo> {
    require_super_admin(auth_user, "update users")?;

    let password_hash = match &req.password {
        Some(pw) => {
            if pw.len() < 8 {
                return Err(AppError::BadRequest(
                    "Password must be at least 8 characters".to_string(),
                ));
            }
            let pw = pw.clone();
            Some(
                tokio::task::spawn_blocking(move || bcrypt::hash(pw, 10))
                    .await
                    .map_err(|_| AppError::Internal("Password hashing failed".to_string()))??,
            )
        }
        None => None,
    };

    let user = sqlx::query_as::<_, User>(
        r#"UPDATE users SET
           username = COALESCE($2, username),
           password_hash = COALESCE($3, password_hash),
           role = COALESCE($4, role),
           tenant_id = COALESCE($5, tenant_id),
           email = COALESCE($6, email),
           is_active = COALESCE($7, is_active),
           updated_at = NOW()
           WHERE id = $1
           RETURNING *"#,
    )
    .bind(id)
    .bind(&req.username)
    .bind(&password_hash)
    .bind(&req.role)
    .bind(req.tenant_id)
    .bind(&req.email)
    .bind(req.is_active)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

    Ok(UserInfo::from(user))
}

/// Delete a user by ID (super_admin only, cannot delete self).
pub async fn delete(pool: &PgPool, auth_user: &AuthUser, id: Uuid) -> AppResult<()> {
    require_super_admin(auth_user, "delete users")?;

    if auth_user.user_id == id {
        return Err(AppError::BadRequest("Cannot delete yourself".to_string()));
    }

    let result = sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("User not found".to_string()));
    }

    Ok(())
}

/// Invite a user by email (super_admin only).
/// Pre-creates a user record so OAuth login can match by email.
pub async fn invite(
    pool: &PgPool,
    auth_user: &AuthUser,
    req: InviteUserRequest,
) -> AppResult<UserInfo> {
    require_super_admin(auth_user, "invite users")?;

    let email = req.email.trim().to_lowercase();
    if email.is_empty() || !email.contains('@') {
        return Err(AppError::BadRequest(
            "Valid email is required".to_string(),
        ));
    }

    let role = req.role.as_deref().unwrap_or("member");
    if role != "super_admin" && role != "member" {
        return Err(AppError::BadRequest(
            "Role must be 'super_admin' or 'member'".to_string(),
        ));
    }

    let user = sqlx::query_as::<_, User>(
        r#"INSERT INTO users (username, email, role, tenant_id, auth_method)
           VALUES ($1, $2, $3, $4, 'invited')
           RETURNING *"#,
    )
    .bind(&email)
    .bind(&email)
    .bind(role)
    .bind(req.tenant_id)
    .fetch_one(pool)
    .await
    .map_err(|e| {
        if let sqlx::Error::Database(ref db_err) = e
            && db_err.constraint() == Some("users_username_key")
        {
            return AppError::Conflict("A user with this email already exists".to_string());
        }
        AppError::Database(e)
    })?;

    Ok(UserInfo::from(user))
}
