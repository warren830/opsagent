use axum::{extract::State, http::header::SET_COOKIE, Json};
use axum::response::{IntoResponse, Response};
use chrono::Utc;
use jsonwebtoken::{encode, EncodingKey, Header};

use crate::error::{AppError, AppResult};
use crate::middleware::auth::{AuthUser, Claims};
use crate::models::user::{
    ChangePasswordRequest, LoginRequest, LoginResponse, UserInfo,
};
use crate::AppState;

/// POST /api/auth/login
pub async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> AppResult<Response> {
    // Find user by username
    let user = sqlx::query_as::<_, crate::models::user::User>(
        "SELECT * FROM users WHERE username = $1 AND is_active = true",
    )
    .bind(&req.username)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::Unauthorized("Invalid credentials".to_string()))?;

    // Verify password (use spawn_blocking for bcrypt)
    let password = req.password.clone();
    let hash = user.password_hash.clone();
    let valid = tokio::task::spawn_blocking(move || bcrypt::verify(password, &hash))
        .await
        .map_err(|_| AppError::Internal("Password verification failed".to_string()))?
        .map_err(|_| AppError::Unauthorized("Invalid credentials".to_string()))?;

    if !valid {
        return Err(AppError::Unauthorized("Invalid credentials".to_string()));
    }

    // Update last login
    sqlx::query("UPDATE users SET last_login_at = NOW() WHERE id = $1")
        .bind(user.id)
        .execute(&state.pool)
        .await?;

    // Generate JWT
    let now = Utc::now().timestamp() as usize;
    let claims = Claims {
        sub: user.id,
        role: user.role.clone(),
        tenant_id: user.tenant_id,
        username: user.username.clone(),
        iat: now,
        exp: now + 86400, // 24 hours
    };

    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(state.config.jwt_secret.as_bytes()),
    )?;

    let user_info: UserInfo = user.into();

    // Set HttpOnly cookie
    let cookie_value = if state.config.env.is_prod() {
        format!(
            "token={}; HttpOnly; Secure; SameSite=Strict; Path=/; Max-Age=86400",
            token
        )
    } else {
        format!(
            "token={}; HttpOnly; SameSite=Lax; Path=/; Max-Age=86400",
            token
        )
    };

    let body = Json(LoginResponse {
        user: user_info,
        token: token.clone(),
    });

    let mut response = body.into_response();
    response
        .headers_mut()
        .insert(SET_COOKIE, cookie_value.parse().unwrap());

    Ok(response)
}

/// POST /api/auth/logout
pub async fn logout() -> Response {
    let cookie = "token=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0";
    let mut response =
        Json(serde_json::json!({"message": "Logged out"})).into_response();
    response
        .headers_mut()
        .insert(SET_COOKIE, cookie.parse().unwrap());
    response
}

/// GET /api/auth/me
pub async fn me(auth_user: axum::Extension<AuthUser>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "id": auth_user.user_id,
        "username": auth_user.username,
        "role": auth_user.role,
        "tenant_id": auth_user.tenant_id,
    }))
}

/// PUT /api/auth/change-password
pub async fn change_password(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Json(req): Json<ChangePasswordRequest>,
) -> AppResult<Json<serde_json::Value>> {
    if req.new_password.len() < 8 {
        return Err(AppError::BadRequest(
            "Password must be at least 8 characters".to_string(),
        ));
    }

    // Verify current password
    let user = sqlx::query_as::<_, crate::models::user::User>(
        "SELECT * FROM users WHERE id = $1",
    )
    .bind(auth_user.user_id)
    .fetch_one(&state.pool)
    .await?;

    let current_pw = req.current_password.clone();
    let hash = user.password_hash.clone();
    let valid = tokio::task::spawn_blocking(move || bcrypt::verify(current_pw, &hash))
        .await
        .map_err(|_| AppError::Internal("Password verification failed".to_string()))?
        .map_err(|_| AppError::Unauthorized("Current password is incorrect".to_string()))?;

    if !valid {
        return Err(AppError::Unauthorized(
            "Current password is incorrect".to_string(),
        ));
    }

    // Hash and update
    let new_pw = req.new_password.clone();
    let new_hash = tokio::task::spawn_blocking(move || bcrypt::hash(new_pw, 10))
        .await
        .map_err(|_| AppError::Internal("Password hashing failed".to_string()))??;

    sqlx::query("UPDATE users SET password_hash = $1, updated_at = NOW() WHERE id = $2")
        .bind(&new_hash)
        .bind(auth_user.user_id)
        .execute(&state.pool)
        .await?;

    Ok(Json(
        serde_json::json!({"message": "Password changed successfully"}),
    ))
}
