//! HTTP-level tests for protected auth endpoints: `/api/auth/me` + 401 matrix.
//! `login` requires `ConnectInfo<SocketAddr>` which doesn't compose with `oneshot`,
//! so login itself is covered by service_user_test + E2E. This file guards the
//! auth middleware + `me` contract that every other handler depends on.

mod helpers;

use axum::{Router, http::StatusCode, routing::get};
use helpers::http::{body_json, call, get as http_get, test_state, token_member, token_expired, with_auth};
use ops::AppState;
use ops::handlers;
use sqlx::PgPool;
use uuid::Uuid;

fn auth_router() -> Router<AppState> {
    with_auth(Router::new().route("/api/auth/me", get(handlers::auth::me)))
}

async fn app(pool: PgPool) -> Router {
    auth_router().with_state(test_state(pool))
}

#[sqlx::test(migrations = "src/migrations")]
async fn me_returns_claims_when_authed(pool: PgPool) {
    let tenant_id = Uuid::new_v4();
    let token = token_member(tenant_id);
    let resp = call(app(pool).await, http_get("/api/auth/me", Some(&token))).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    assert_eq!(body["role"], "member");
    assert_eq!(body["tenant_id"], tenant_id.to_string());
    assert_eq!(body["username"], "test_member");
}

#[sqlx::test(migrations = "src/migrations")]
async fn me_rejects_missing_token(pool: PgPool) {
    let resp = call(app(pool).await, http_get("/api/auth/me", None)).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "src/migrations")]
async fn me_rejects_malformed_token(pool: PgPool) {
    let resp = call(app(pool).await, http_get("/api/auth/me", Some("not.a.jwt"))).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "src/migrations")]
async fn me_rejects_expired_token(pool: PgPool) {
    let resp = call(app(pool).await, http_get("/api/auth/me", Some(&token_expired()))).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "src/migrations")]
async fn me_rejects_token_signed_with_wrong_secret(pool: PgPool) {
    use jsonwebtoken::{EncodingKey, Header, encode};
    use ops::middleware::auth::Claims;
    let now = chrono::Utc::now().timestamp() as usize;
    let bad = encode(
        &Header::default(),
        &Claims {
            sub: Uuid::new_v4(),
            role: "member".into(),
            tenant_id: None,
            username: "x".into(),
            token_type: "access".into(),
            iat: now,
            exp: now + 3600,
        },
        &EncodingKey::from_secret(b"wrong-secret-wrong-secret-wrong!"),
    )
    .unwrap();
    let resp = call(app(pool).await, http_get("/api/auth/me", Some(&bad))).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}
