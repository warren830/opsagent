//! HTTP tests for /api/skills — inline skill CRUD (git-based skills require a
//! real repo clone and are covered by E2E).

mod helpers;

use axum::{Router, http::StatusCode, routing::{get, post, put}};
use helpers::http::{
    body_json, call, delete, get as http_get, issue_token, json_request, test_config, with_auth,
};
use ops::AppState;
use ops::handlers;
use serde_json::json;
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

/// Build AppState with an isolated work_dir under /tmp so create_inline can
/// actually write skill directories without polluting the repo.
fn state_with_tmpdir(pool: PgPool) -> AppState {
    let mut config = test_config();
    config.claude_work_dir = format!("/tmp/ops-test-skills-{}", Uuid::new_v4());
    AppState {
        pool,
        config,
        rca_registry: Arc::new(ops::services::rca::RcaRegistry::new()),
    }
}

fn app(pool: PgPool) -> Router {
    with_auth(
        Router::new()
            .route("/api/skills", get(handlers::skill::list))
            .route("/api/skills/inline", post(handlers::skill::create_inline))
            .route("/api/skills/{id}/inline", put(handlers::skill::update_inline))
            .route(
                "/api/skills/{id}",
                axum::routing::delete(handlers::skill::delete),
            ),
    )
    .with_state(state_with_tmpdir(pool))
}

async fn seed_tenant(pool: &PgPool, slug: &str) -> Uuid {
    sqlx::query_scalar::<_, Uuid>("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind(slug)
        .bind(slug)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed_user(pool: &PgPool, role: &str, tid: Option<Uuid>) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO users (username, password_hash, role, tenant_id) VALUES ($1, 'x', $2, $3) RETURNING id",
    )
    .bind(format!("u-{}", Uuid::new_v4()))
    .bind(role)
    .bind(tid)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn create_inline(app: Router, token: &str, name: &str) -> Uuid {
    let resp = call(
        app,
        json_request(
            "POST",
            "/api/skills/inline",
            Some(token),
            &json!({"name": name, "instructions": "do the thing", "visibility": "public"}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK, "create_inline failed: {:?}", resp.status());
    Uuid::parse_str(body_json(resp).await["id"].as_str().unwrap()).unwrap()
}

#[sqlx::test(migrations = "src/migrations")]
async fn create_inline_rejects_empty_name(pool: PgPool) {
    let t = seed_tenant(&pool, "t1").await;
    let uid = seed_user(&pool, "tenant_admin", Some(t)).await;
    let token = issue_token(uid, "tenant_admin", Some(t), "u");
    let resp = call(
        app(pool),
        json_request(
            "POST",
            "/api/skills/inline",
            Some(&token),
            &json!({"name": "", "instructions": "x"}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "src/migrations")]
async fn create_inline_rejects_empty_instructions(pool: PgPool) {
    let t = seed_tenant(&pool, "t1").await;
    let uid = seed_user(&pool, "tenant_admin", Some(t)).await;
    let token = issue_token(uid, "tenant_admin", Some(t), "u");
    let resp = call(
        app(pool),
        json_request(
            "POST",
            "/api/skills/inline",
            Some(&token),
            &json!({"name": "valid", "instructions": ""}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "src/migrations")]
async fn create_inline_assigns_tenant_and_visibility(pool: PgPool) {
    let t = seed_tenant(&pool, "t1").await;
    let uid = seed_user(&pool, "tenant_admin", Some(t)).await;
    let token = issue_token(uid, "tenant_admin", Some(t), "u");
    let resp = call(
        app(pool),
        json_request(
            "POST",
            "/api/skills/inline",
            Some(&token),
            &json!({"name": "my-skill", "instructions": "do it", "visibility": "public"}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let v = body_json(resp).await;
    assert_eq!(v["tenant_id"], t.to_string());
    assert_eq!(v["visibility"], "public");
    assert_eq!(v["name"], "my-skill");
}

#[sqlx::test(migrations = "src/migrations")]
async fn private_skill_binds_to_creator_user_id(pool: PgPool) {
    let t = seed_tenant(&pool, "t1").await;
    let uid = seed_user(&pool, "member", Some(t)).await;
    let token = issue_token(uid, "member", Some(t), "u");
    let resp = call(
        app(pool),
        json_request(
            "POST",
            "/api/skills/inline",
            Some(&token),
            &json!({"name": "mine", "instructions": "x", "visibility": "private"}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(body_json(resp).await["user_id"], uid.to_string());
}

#[sqlx::test(migrations = "src/migrations")]
async fn update_inline_hot_reloads_instructions(pool: PgPool) {
    let t = seed_tenant(&pool, "t1").await;
    let uid = seed_user(&pool, "tenant_admin", Some(t)).await;
    let token = issue_token(uid, "tenant_admin", Some(t), "u");
    let id = create_inline(app(pool.clone()), &token, "update-me").await;

    let resp = call(
        app(pool),
        json_request(
            "PUT",
            &format!("/api/skills/{id}/inline"),
            Some(&token),
            &json!({"instructions": "new body"}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(body_json(resp).await["instructions"], "new body");
}

#[sqlx::test(migrations = "src/migrations")]
async fn list_returns_tenant_skills(pool: PgPool) {
    let t = seed_tenant(&pool, "t1").await;
    let uid = seed_user(&pool, "tenant_admin", Some(t)).await;
    let token = issue_token(uid, "tenant_admin", Some(t), "u");
    create_inline(app(pool.clone()), &token, "s1").await;
    create_inline(app(pool.clone()), &token, "s2").await;

    let resp = call(app(pool), http_get("/api/skills", Some(&token))).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(body_json(resp).await.as_array().unwrap().len() >= 2);
}

#[sqlx::test(migrations = "src/migrations")]
async fn delete_own_skill(pool: PgPool) {
    let t = seed_tenant(&pool, "t1").await;
    let uid = seed_user(&pool, "tenant_admin", Some(t)).await;
    let token = issue_token(uid, "tenant_admin", Some(t), "u");
    let id = create_inline(app(pool.clone()), &token, "gone").await;
    let resp = call(app(pool), delete(&format!("/api/skills/{id}"), Some(&token))).await;
    assert_eq!(resp.status(), StatusCode::OK);
}
