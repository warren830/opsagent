use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;

/// Unified application error type.
/// All handlers return `Result<T, AppError>`.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("JWT error: {0}")]
    Jwt(#[from] jsonwebtoken::errors::Error),

    #[error("Bcrypt error: {0}")]
    Bcrypt(#[from] bcrypt::BcryptError),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("OAuth error: {0}")]
    OAuth(String),

    #[error("Token theft detected")]
    TokenTheft,

    #[error("HTTP client error: {0}")]
    HttpClient(String),

    #[error("Kubernetes API error: {0}")]
    Kubernetes(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            AppError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg.clone()),
            AppError::Forbidden(msg) => (StatusCode::FORBIDDEN, msg.clone()),
            AppError::Conflict(msg) => (StatusCode::CONFLICT, msg.clone()),
            AppError::Internal(msg) => {
                tracing::error!("Internal error: {}", msg);
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".to_string())
            }
            AppError::Database(err) => {
                tracing::error!("Database error: {}", err);
                (StatusCode::INTERNAL_SERVER_ERROR, "Database error".to_string())
            }
            AppError::Jwt(err) => {
                tracing::warn!("JWT error: {}", err);
                (StatusCode::UNAUTHORIZED, "Invalid or expired token".to_string())
            }
            AppError::Bcrypt(err) => {
                tracing::error!("Bcrypt error: {}", err);
                (StatusCode::INTERNAL_SERVER_ERROR, "Authentication error".to_string())
            }
            AppError::Json(err) => {
                tracing::warn!("JSON parse error: {}", err);
                (StatusCode::BAD_REQUEST, format!("Invalid JSON: {}", err))
            }
            AppError::OAuth(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            AppError::TokenTheft => {
                tracing::warn!("Token theft detected — revoking family");
                (
                    StatusCode::UNAUTHORIZED,
                    "Token theft detected. Please re-login.".to_string(),
                )
            }
            AppError::HttpClient(msg) => {
                tracing::error!("HTTP client error: {}", msg);
                (StatusCode::BAD_GATEWAY, "External service error".to_string())
            }
            AppError::Kubernetes(msg) => {
                tracing::error!("Kubernetes API error: {}", msg);
                (StatusCode::BAD_GATEWAY, "Kubernetes API error".to_string())
            }
        };

        let body = json!({
            "error": message,
            "status": status.as_u16(),
        });

        (status, Json(body)).into_response()
    }
}

/// Result type alias for handlers
pub type AppResult<T> = Result<T, AppError>;
