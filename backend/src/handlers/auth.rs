use axum::response::{IntoResponse, Response};
use axum::{Json, extract::State, http::header::SET_COOKIE};

use crate::AppState;
use crate::error::{AppError, AppResult};
use crate::middleware::auth::AuthUser;
use crate::models::user::{ChangePasswordRequest, LoginRequest, LoginResponse, UserInfo};
use crate::services::{auth_common, refresh_token};

/// POST /api/auth/login
pub async fn login(
    State(state): State<AppState>,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<std::net::SocketAddr>,
    headers: axum::http::HeaderMap,
    Json(req): Json<LoginRequest>,
) -> AppResult<Response> {
    // Find user by username
    let user =
        sqlx::query_as::<_, crate::models::user::User>("SELECT * FROM users WHERE username = $1 AND is_active = true")
            .bind(&req.username)
            .fetch_optional(&state.pool)
            .await?
            .ok_or_else(|| AppError::Unauthorized("Invalid credentials".to_string()))?;

    // Verify password (use spawn_blocking for bcrypt)
    let password = req.password.clone();
    let hash = user
        .password_hash
        .clone()
        .ok_or_else(|| AppError::Unauthorized("This account uses OAuth login".to_string()))?;
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

    // Generate access token (configurable expiry)
    let token = auth_common::create_access_token(&state.config, &user)?;

    // Generate refresh token
    let ip = addr.ip().to_string();
    let ua = headers
        .get(http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let (refresh_jwt, _) =
        refresh_token::create_refresh_token(&state.pool, &state.config, user.id, None, None, Some(&ip), Some(ua))
            .await?;

    let user_info: UserInfo = user.into();

    // Set HttpOnly cookies (access + refresh)
    let access_max_age = state.config.jwt_access_token_expire_minutes * 60;
    let access_cookie = if state.config.env.is_prod() {
        format!(
            "token={}; HttpOnly; Secure; SameSite=Strict; Path=/; Max-Age={}",
            token, access_max_age
        )
    } else {
        format!(
            "token={}; HttpOnly; SameSite=Lax; Path=/; Max-Age={}",
            token, access_max_age
        )
    };
    let refresh_cookie = auth_common::refresh_token_cookie(&state.config, &refresh_jwt);

    let body = Json(LoginResponse {
        user: user_info,
        token: token.clone(),
    });

    let mut response = body.into_response();
    response
        .headers_mut()
        .append(SET_COOKIE, access_cookie.parse().unwrap());
    response
        .headers_mut()
        .append(SET_COOKIE, refresh_cookie.parse().unwrap());

    Ok(response)
}

/// POST /api/auth/logout
pub async fn logout(State(state): State<AppState>, headers: axum::http::HeaderMap) -> Response {
    // Revoke refresh token if present
    let cookie_str = headers.get(http::header::COOKIE).and_then(|v| v.to_str().ok());

    if let Some(refresh_jwt) = auth_common::extract_refresh_token_from_cookie(cookie_str) {
        let token_hash = refresh_token::hash_refresh_token(&refresh_jwt, &state.config.jwt_secret);
        let _ = refresh_token::revoke_by_hash(&state.pool, &token_hash, "logout").await;
    }

    // Clear both cookies
    let clear_access = "token=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0";
    let clear_refresh = auth_common::clear_refresh_token_cookie(&state.config);

    let mut response = Json(serde_json::json!({"message": "Logged out"})).into_response();
    response.headers_mut().append(SET_COOKIE, clear_access.parse().unwrap());
    response
        .headers_mut()
        .append(SET_COOKIE, clear_refresh.parse().unwrap());
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
    let user = sqlx::query_as::<_, crate::models::user::User>("SELECT * FROM users WHERE id = $1")
        .bind(auth_user.user_id)
        .fetch_one(&state.pool)
        .await?;

    let current_pw = req.current_password.clone();
    let hash = user
        .password_hash
        .clone()
        .ok_or_else(|| AppError::BadRequest("This account uses OAuth — no password to change".to_string()))?;
    let valid = tokio::task::spawn_blocking(move || bcrypt::verify(current_pw, &hash))
        .await
        .map_err(|_| AppError::Internal("Password verification failed".to_string()))?
        .map_err(|_| AppError::Unauthorized("Current password is incorrect".to_string()))?;

    if !valid {
        return Err(AppError::Unauthorized("Current password is incorrect".to_string()));
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

    Ok(Json(serde_json::json!({"message": "Password changed successfully"})))
}
