//! HTTP test helpers — spawn a minimal Axum test router, issue JWTs, send requests.
//!
//! Each handler test module registers only the routes it needs via `test_router()`.

use axum::{Router, body::Body, http::Request, response::Response};
use jsonwebtoken::{EncodingKey, Header, encode};
use ops::config::{AppConfig, Environment};
use ops::middleware::auth::{Claims, auth_middleware};
use ops::AppState;
use serde::Serialize;
use sqlx::PgPool;
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

pub const TEST_JWT_SECRET: &str = "test-secret-minimum-32-characters-long-test-only";

/// Build a minimal AppConfig for tests.
pub fn test_config() -> AppConfig {
    AppConfig {
        env: Environment::Local,
        backend_port: 0,
        database_url: String::new(),
        db_max_connections: 1,
        db_min_connections: 1,
        jwt_secret: TEST_JWT_SECRET.to_string(),
        jwt_access_token_expire_minutes: 30,
        jwt_refresh_token_expire_days: 7,
        allowed_origins: vec!["http://localhost:3003".to_string()],
        claude_bin: "echo".to_string(),
        claude_timeout_ms: 1000,
        claude_model: "test".to_string(),
        claude_work_dir: "/tmp".to_string(),
        aws_region: "us-east-1".to_string(),
        microsoft_oauth: None,
        cognito_oauth: None,
    }
}

/// Build `AppState` for tests from a pool.
pub fn test_state(pool: PgPool) -> AppState {
    AppState {
        pool,
        config: test_config(),
        rca_registry: Arc::new(ops::services::rca::RcaRegistry::new()),
    }
}

/// Attach auth middleware to a protected sub-router.
pub fn with_auth(protected: Router<AppState>) -> Router<AppState> {
    protected.layer(axum::middleware::from_fn_with_state(
        TEST_JWT_SECRET.to_string(),
        auth_middleware,
    ))
}

/// Issue a JWT for a user with the given role + optional tenant.
pub fn issue_token(user_id: Uuid, role: &str, tenant_id: Option<Uuid>, username: &str) -> String {
    let now = chrono::Utc::now().timestamp() as usize;
    let claims = Claims {
        sub: user_id,
        role: role.to_string(),
        tenant_id,
        username: username.to_string(),
        token_type: "access".to_string(),
        iat: now,
        exp: now + 3600,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(TEST_JWT_SECRET.as_bytes()),
    )
    .expect("encode test JWT")
}

/// Convenience: token for a super admin.
pub fn token_super_admin() -> String {
    issue_token(Uuid::new_v4(), "super_admin", None, "test_super")
}

/// Convenience: token for a tenant_admin bound to `tenant_id`.
pub fn token_tenant_admin(tenant_id: Uuid) -> String {
    issue_token(Uuid::new_v4(), "tenant_admin", Some(tenant_id), "test_tadmin")
}

/// Convenience: token for a regular member of a tenant.
pub fn token_member(tenant_id: Uuid) -> String {
    issue_token(Uuid::new_v4(), "member", Some(tenant_id), "test_member")
}

/// Issue an expired token (for 401-on-expired tests).
pub fn token_expired() -> String {
    let claims = Claims {
        sub: Uuid::new_v4(),
        role: "member".to_string(),
        tenant_id: None,
        username: "expired".to_string(),
        token_type: "access".to_string(),
        iat: 1_000_000_000,
        exp: 1_000_000_100,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(TEST_JWT_SECRET.as_bytes()),
    )
    .unwrap()
}

/// Build a GET request with optional bearer token.
pub fn get(uri: &str, bearer: Option<&str>) -> Request<Body> {
    let mut b = Request::builder().method("GET").uri(uri);
    if let Some(t) = bearer {
        b = b.header("Authorization", format!("Bearer {t}"));
    }
    b.body(Body::empty()).unwrap()
}

/// Build a JSON POST/PUT/DELETE request with optional bearer token.
pub fn json_request<T: Serialize>(method: &str, uri: &str, bearer: Option<&str>, body: &T) -> Request<Body> {
    let mut b = Request::builder()
        .method(method)
        .uri(uri)
        .header("Content-Type", "application/json");
    if let Some(t) = bearer {
        b = b.header("Authorization", format!("Bearer {t}"));
    }
    b.body(Body::from(serde_json::to_vec(body).unwrap())).unwrap()
}

/// Convenience: DELETE with bearer.
pub fn delete(uri: &str, bearer: Option<&str>) -> Request<Body> {
    let mut b = Request::builder().method("DELETE").uri(uri);
    if let Some(t) = bearer {
        b = b.header("Authorization", format!("Bearer {t}"));
    }
    b.body(Body::empty()).unwrap()
}

/// Send a request to the router and return the response.
pub async fn call(app: Router, req: Request<Body>) -> Response {
    app.oneshot(req).await.unwrap()
}

/// Read response body as bytes.
pub async fn body_bytes(resp: Response) -> bytes::Bytes {
    use http_body_util::BodyExt;
    resp.into_body().collect().await.unwrap().to_bytes()
}

/// Read response body as JSON value.
pub async fn body_json(resp: Response) -> serde_json::Value {
    let b = body_bytes(resp).await;
    serde_json::from_slice(&b).unwrap_or(serde_json::Value::Null)
}
