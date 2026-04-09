use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};
use jsonwebtoken::{DecodingKey, Validation, decode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::AppError;

/// Default token type for backward compatibility with existing tokens
fn default_access() -> String {
    "access".to_string()
}

/// JWT claims stored in the token
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: Uuid,    // user_id
    pub role: String, // super_admin / tenant_admin
    pub tenant_id: Option<Uuid>,
    pub username: String,
    #[serde(default = "default_access")]
    pub token_type: String, // "access" or "refresh"
    pub exp: usize, // expiration timestamp
    pub iat: usize, // issued at
}

/// Authenticated user info extracted from JWT, available to handlers
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: Uuid,
    pub role: String,
    pub tenant_id: Option<Uuid>,
    pub username: String,
}

impl AuthUser {
    pub fn is_super_admin(&self) -> bool {
        self.role == "super_admin"
    }

    pub fn is_tenant_admin(&self) -> bool {
        self.role == "tenant_admin"
    }

    /// Super admin or tenant admin — can manage account access
    pub fn is_admin(&self) -> bool {
        self.is_super_admin() || self.is_tenant_admin()
    }

    /// Check if user has access to a specific tenant's resources
    pub fn can_access_tenant(&self, tenant_id: &Uuid) -> bool {
        self.is_super_admin() || self.tenant_id.as_ref() == Some(tenant_id)
    }
}

/// Extract JWT from cookie or Authorization header
fn extract_token(request: &Request) -> Option<String> {
    // Try cookie first
    if let Some(cookie_header) = request.headers().get(http::header::COOKIE)
        && let Ok(cookie_str) = cookie_header.to_str()
    {
        for cookie in cookie_str.split(';') {
            let cookie = cookie.trim();
            if let Some(token) = cookie.strip_prefix("token=") {
                return Some(token.to_string());
            }
        }
    }

    // Fall back to Authorization: Bearer <token>
    if let Some(auth_header) = request.headers().get(http::header::AUTHORIZATION)
        && let Ok(auth_str) = auth_header.to_str()
        && let Some(token) = auth_str.strip_prefix("Bearer ")
    {
        return Some(token.to_string());
    }

    None
}

/// Authentication middleware — validates JWT and injects AuthUser into request extensions
pub async fn auth_middleware(
    State(jwt_secret): State<String>,
    mut request: Request,
    next: Next,
) -> Result<Response, AppError> {
    let token =
        extract_token(&request).ok_or_else(|| AppError::Unauthorized("Missing authentication token".to_string()))?;

    let token_data = decode::<Claims>(
        &token,
        &DecodingKey::from_secret(jwt_secret.as_bytes()),
        &Validation::default(),
    )
    .map_err(|e| AppError::Unauthorized(format!("Invalid token: {}", e)))?;

    // Reject refresh tokens used as access tokens
    if token_data.claims.token_type != "access" {
        return Err(AppError::Unauthorized("Invalid token type".to_string()));
    }

    let auth_user = AuthUser {
        user_id: token_data.claims.sub,
        role: token_data.claims.role,
        tenant_id: token_data.claims.tenant_id,
        username: token_data.claims.username,
    };

    request.extensions_mut().insert(auth_user);
    Ok(next.run(request).await)
}
