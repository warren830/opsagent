use axum::Json;
use axum::extract::State;
use axum::http::header::SET_COOKIE;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::error::{AppError, AppResult};
use crate::middleware::auth::AuthUser;
use crate::services::{account_linking, auth_common, cognito_oauth, microsoft_oauth, oauth_state, refresh_token};

// ─── GET /api/auth/providers ────────────────────────────────────────

#[derive(Serialize)]
pub struct ProvidersResponse {
    pub local: bool,
    pub microsoft: bool,
    pub cognito: bool,
    pub is_cloud: bool,
}

pub async fn providers(State(state): State<AppState>) -> Json<ProvidersResponse> {
    Json(ProvidersResponse {
        // Local login is always available when no OAuth is configured,
        // or always available in local mode
        local: state.config.env.is_local()
            || (!state.config.microsoft_is_configured() && !state.config.cognito_is_configured()),
        microsoft: state.config.microsoft_is_configured(),
        cognito: state.config.cognito_is_configured(),
        is_cloud: !state.config.env.is_local(),
    })
}

// ─── GET /api/auth/microsoft/login ──────────────────────────────────

#[derive(Deserialize)]
pub struct OAuthLoginQuery {
    pub redirect_uri: Option<String>,
}

#[derive(Serialize)]
pub struct OAuthLoginResponse {
    pub auth_url: String,
    pub state: String,
}

pub async fn microsoft_login(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<OAuthLoginQuery>,
) -> AppResult<Json<OAuthLoginResponse>> {
    let ms_config = state
        .config
        .microsoft_oauth
        .as_ref()
        .ok_or_else(|| AppError::OAuth("Microsoft OAuth not configured".to_string()))?;

    // Determine redirect_uri (from query param or first configured one)
    let redirect_uri = query
        .redirect_uri
        .or_else(|| ms_config.redirect_uris.first().cloned())
        .ok_or_else(|| AppError::OAuth("No redirect URI configured for Microsoft".to_string()))?;

    let (oauth_state, code_challenge) =
        oauth_state::create_oauth_state(&state.pool, "microsoft", Some(&redirect_uri)).await?;

    let auth_url = microsoft_oauth::get_authorization_url(ms_config, &redirect_uri, &oauth_state, &code_challenge);

    Ok(Json(OAuthLoginResponse {
        auth_url,
        state: oauth_state,
    }))
}

// ─── POST /api/auth/microsoft/callback ──────────────────────────────

#[derive(Deserialize)]
pub struct OAuthCallbackRequest {
    pub code: String,
    pub state: String,
}

pub async fn microsoft_callback(
    State(state): State<AppState>,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<std::net::SocketAddr>,
    headers: axum::http::HeaderMap,
    Json(req): Json<OAuthCallbackRequest>,
) -> AppResult<Response> {
    let ms_config = state
        .config
        .microsoft_oauth
        .as_ref()
        .ok_or_else(|| AppError::OAuth("Microsoft OAuth not configured".to_string()))?;

    // Validate state and get code_verifier
    let (code_verifier, redirect_uri) =
        oauth_state::validate_and_consume_state(&state.pool, &req.state, "microsoft").await?;

    let redirect_uri = redirect_uri
        .or_else(|| ms_config.redirect_uris.first().cloned())
        .ok_or_else(|| AppError::OAuth("No redirect URI available".to_string()))?;

    // Exchange code for token
    let token_resp =
        microsoft_oauth::exchange_code_for_token(ms_config, &req.code, &redirect_uri, &code_verifier).await?;

    tracing::debug!(
        provider = "microsoft",
        token_type = %token_resp.token_type,
        expires_in = token_resp.expires_in,
        has_id_token = token_resp.id_token.is_some(),
        "OAuth token exchange completed"
    );

    // Get user info
    let user_info = microsoft_oauth::get_user_info(&token_resp.access_token).await?;

    // Find or create user
    let user = account_linking::find_or_create_oauth_user(
        &state.pool,
        "microsoft",
        &user_info.id,
        user_info.email().as_deref(),
        &user_info.display_name_or_email(),
    )
    .await?;

    // Issue tokens
    let ip = addr.ip().to_string();
    let ua = headers
        .get(http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    issue_token_response(&state, &user, Some(&ip), Some(ua)).await
}

// ─── GET /api/auth/cognito/login ────────────────────────────────────

pub async fn cognito_login(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<OAuthLoginQuery>,
) -> AppResult<Json<OAuthLoginResponse>> {
    let cog_config = state
        .config
        .cognito_oauth
        .as_ref()
        .ok_or_else(|| AppError::OAuth("Cognito OAuth not configured".to_string()))?;

    let redirect_uri = query
        .redirect_uri
        .or_else(|| cog_config.redirect_uris.first().cloned())
        .ok_or_else(|| AppError::OAuth("No redirect URI configured for Cognito".to_string()))?;

    let (oauth_state, code_challenge) =
        oauth_state::create_oauth_state(&state.pool, "cognito", Some(&redirect_uri)).await?;

    let auth_url = cognito_oauth::get_authorization_url(cog_config, &redirect_uri, &oauth_state, &code_challenge);

    Ok(Json(OAuthLoginResponse {
        auth_url,
        state: oauth_state,
    }))
}

// ─── POST /api/auth/cognito/callback ────────────────────────────────

pub async fn cognito_callback(
    State(state): State<AppState>,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<std::net::SocketAddr>,
    headers: axum::http::HeaderMap,
    Json(req): Json<OAuthCallbackRequest>,
) -> AppResult<Response> {
    let cog_config = state
        .config
        .cognito_oauth
        .as_ref()
        .ok_or_else(|| AppError::OAuth("Cognito OAuth not configured".to_string()))?;

    let (code_verifier, redirect_uri) =
        oauth_state::validate_and_consume_state(&state.pool, &req.state, "cognito").await?;

    let redirect_uri = redirect_uri
        .or_else(|| cog_config.redirect_uris.first().cloned())
        .ok_or_else(|| AppError::OAuth("No redirect URI available".to_string()))?;

    let token_resp =
        cognito_oauth::exchange_code_for_token(cog_config, &req.code, &redirect_uri, &code_verifier).await?;

    tracing::debug!(
        provider = "cognito",
        token_type = %token_resp.token_type,
        expires_in = token_resp.expires_in,
        has_id_token = token_resp.id_token.is_some(),
        "OAuth token exchange completed"
    );

    let user_info = cognito_oauth::get_user_info(cog_config, &token_resp.access_token).await?;

    let user = account_linking::find_or_create_oauth_user(
        &state.pool,
        "cognito",
        &user_info.sub,
        user_info.email.as_deref(),
        &user_info.display_name_or_email(),
    )
    .await?;

    let ip = addr.ip().to_string();
    let ua = headers
        .get(http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    issue_token_response(&state, &user, Some(&ip), Some(ua)).await
}

// ─── POST /api/auth/refresh ─────────────────────────────────────────

pub async fn refresh(
    State(state): State<AppState>,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<std::net::SocketAddr>,
    headers: axum::http::HeaderMap,
) -> AppResult<Response> {
    let cookie_str = headers.get(http::header::COOKIE).and_then(|v| v.to_str().ok());

    let refresh_jwt = auth_common::extract_refresh_token_from_cookie(cookie_str)
        .ok_or_else(|| AppError::Unauthorized("No refresh token".to_string()))?;

    let ip = addr.ip().to_string();
    let ua = headers
        .get(http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let (access_jwt, new_refresh_jwt, user) =
        refresh_token::validate_and_rotate(&state.pool, &state.config, &refresh_jwt, Some(&ip), Some(ua)).await?;

    let user_info: crate::models::user::UserInfo = user.into();

    // Set cookies
    let access_cookie = format!(
        "token={}; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age={}",
        access_jwt,
        state.config.jwt_access_token_expire_minutes * 60
    );
    let refresh_cookie = auth_common::refresh_token_cookie(&state.config, &new_refresh_jwt);

    let body = Json(serde_json::json!({
        "user": user_info,
        "token": access_jwt,
    }));

    let mut response = body.into_response();
    response
        .headers_mut()
        .append(SET_COOKIE, access_cookie.parse().unwrap());
    response
        .headers_mut()
        .append(SET_COOKIE, refresh_cookie.parse().unwrap());

    Ok(response)
}

// ─── POST /api/auth/revoke ──────────────────────────────────────────

pub async fn revoke(State(state): State<AppState>, headers: axum::http::HeaderMap) -> AppResult<Response> {
    let cookie_str = headers.get(http::header::COOKIE).and_then(|v| v.to_str().ok());

    if let Some(refresh_jwt) = auth_common::extract_refresh_token_from_cookie(cookie_str) {
        let token_hash = refresh_token::hash_refresh_token(&refresh_jwt, &state.config.jwt_secret);
        refresh_token::revoke_by_hash(&state.pool, &token_hash, "logout").await?;
    }

    let clear_cookie = auth_common::clear_refresh_token_cookie(&state.config);
    let mut response = Json(serde_json::json!({"message": "Token revoked"})).into_response();
    response.headers_mut().insert(SET_COOKIE, clear_cookie.parse().unwrap());

    Ok(response)
}

// ─── POST /api/auth/revoke-all (protected) ──────────────────────────

pub async fn revoke_all(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
) -> AppResult<Json<serde_json::Value>> {
    let count = refresh_token::revoke_all_user_tokens(&state.pool, auth_user.user_id).await?;
    Ok(Json(serde_json::json!({
        "message": "All tokens revoked",
        "count": count,
    })))
}

// ─── Helper: issue access + refresh tokens ──────────────────────────

async fn issue_token_response(
    state: &AppState,
    user: &crate::models::user::User,
    ip: Option<&str>,
    ua: Option<&str>,
) -> AppResult<Response> {
    let access_jwt = auth_common::create_access_token(&state.config, user)?;

    let (refresh_jwt, _) =
        refresh_token::create_refresh_token(&state.pool, &state.config, user.id, None, None, ip, ua).await?;

    let user_info: crate::models::user::UserInfo = user.clone().into();

    // Set cookies
    let access_cookie = format!(
        "token={}; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age={}",
        access_jwt,
        state.config.jwt_access_token_expire_minutes * 60
    );
    let refresh_cookie = auth_common::refresh_token_cookie(&state.config, &refresh_jwt);

    let body = Json(serde_json::json!({
        "user": user_info,
        "token": access_jwt,
    }));

    let mut response = body.into_response();
    response
        .headers_mut()
        .append(SET_COOKIE, access_cookie.parse().unwrap());
    response
        .headers_mut()
        .append(SET_COOKIE, refresh_cookie.parse().unwrap());

    Ok(response)
}
