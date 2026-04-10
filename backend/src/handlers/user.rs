use axum::{
    Json,
    extract::{Path, State},
};
use uuid::Uuid;

use crate::AppState;
use crate::error::{AppError, AppResult};
use crate::middleware::auth::AuthUser;
use crate::models::user::{CreateUserRequest, InviteUserRequest, UpdateUserRequest, User, UserInfo};

/// GET /api/users
pub async fn list_users(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
) -> AppResult<Json<Vec<UserInfo>>> {
    let users = if auth_user.is_super_admin() {
        sqlx::query_as::<_, User>("SELECT * FROM users ORDER BY username")
            .fetch_all(&state.pool)
            .await?
    } else {
        match auth_user.tenant_id {
            Some(tid) => {
                sqlx::query_as::<_, User>("SELECT * FROM users WHERE tenant_id = $1 ORDER BY username")
                    .bind(tid)
                    .fetch_all(&state.pool)
                    .await?
            }
            None => vec![],
        }
    };

    Ok(Json(users.into_iter().map(UserInfo::from).collect()))
}

/// POST /api/users (super_admin only)
pub async fn create_user(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Json(req): Json<CreateUserRequest>,
) -> AppResult<Json<UserInfo>> {
    if !auth_user.is_super_admin() {
        return Err(AppError::Forbidden("Only super admins can create users".to_string()));
    }

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
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        if let sqlx::Error::Database(ref db_err) = e
            && db_err.constraint() == Some("users_username_key")
        {
            return AppError::Conflict("Username already exists".to_string());
        }
        AppError::Database(e)
    })?;

    Ok(Json(UserInfo::from(user)))
}

/// PUT /api/users/:id
pub async fn update_user(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateUserRequest>,
) -> AppResult<Json<UserInfo>> {
    if !auth_user.is_super_admin() {
        return Err(AppError::Forbidden("Only super admins can update users".to_string()));
    }

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
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

    Ok(Json(UserInfo::from(user)))
}

/// DELETE /api/users/:id (super_admin only)
pub async fn delete_user(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    if !auth_user.is_super_admin() {
        return Err(AppError::Forbidden("Only super admins can delete users".to_string()));
    }

    if auth_user.user_id == id {
        return Err(AppError::BadRequest("Cannot delete yourself".to_string()));
    }

    let result = sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(id)
        .execute(&state.pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("User not found".to_string()));
    }

    Ok(Json(serde_json::json!({"message": "User deleted"})))
}

/// POST /api/users/invite (super_admin only, cloud mode)
/// Pre-creates a user record so OAuth login can match by email.
pub async fn invite_user(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Json(req): Json<InviteUserRequest>,
) -> AppResult<Json<UserInfo>> {
    if !auth_user.is_super_admin() {
        return Err(AppError::Forbidden("Only super admins can invite users".to_string()));
    }

    let email = req.email.trim().to_lowercase();
    if email.is_empty() || !email.contains('@') {
        return Err(AppError::BadRequest("Valid email is required".to_string()));
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
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        if let sqlx::Error::Database(ref db_err) = e
            && db_err.constraint() == Some("users_username_key")
        {
            return AppError::Conflict("A user with this email already exists".to_string());
        }
        AppError::Database(e)
    })?;

    Ok(Json(UserInfo::from(user)))
}
