//! HTTP tests for /api/knowledge — CRUD against the documented "global file"
//! semantics (files with NULL account_id are visible across tenants).
//! Files scoped to an account are filtered via get_accessible_account_ids.

mod helpers;

use axum::{Router, http::StatusCode, routing::get};
use helpers::http::{
    body_json, call, delete, get as http_get, issue_token, json_request, test_state, with_auth,
};
use ops::handlers;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

fn app(pool: PgPool) -> Router {
    with_auth(
        Router::new()
            .route(
                "/api/knowledge",
                get(handlers::knowledge::list).post(handlers::knowledge::create),
            )
            .route(
                "/api/knowledge/{id}",
                axum::routing::put(handlers::knowledge::update).delete(handlers::knowledge::delete),
            ),
    )
    .with_state(test_state(pool))
}

async fn seed_tenant(pool: &PgPool, slug: &str) -> Uuid {
    sqlx::query_scalar::<_, Uuid>("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind(slug)
        .bind(slug)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Seed a user so the FK knowledge_files.created_by can be satisfied.
async fn seed_user(pool: &PgPool, username: &str, role: &str, tid: Option<Uuid>) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO users (username, password_hash, role, tenant_id) VALUES ($1, 'x', $2, $3) RETURNING id",
    )
    .bind(username)
    .bind(role)
    .bind(tid)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn create_file(app: Router, token: &str, name: &str) -> Uuid {
    let resp = call(
        app,
        json_request(
            "POST",
            "/api/knowledge",
            Some(token),
            &json!({"filename": name, "content": "# doc"}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK, "create failed: {:?}", resp.status());
    Uuid::parse_str(body_json(resp).await["id"].as_str().unwrap()).unwrap()
}

#[sqlx::test(migrations = "src/migrations")]
async fn create_rejects_empty_filename(pool: PgPool) {
    let t = seed_tenant(&pool, "t1").await;
    let uid = seed_user(&pool, "u1", "tenant_admin", Some(t)).await;
    let token = issue_token(uid, "tenant_admin", Some(t), "u1");
    let resp = call(
        app(pool),
        json_request(
            "POST",
            "/api/knowledge",
            Some(&token),
            &json!({"filename": "", "content": "x"}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "src/migrations")]
async fn create_global_file_succeeds_and_persists_author(pool: PgPool) {
    let t = seed_tenant(&pool, "t1").await;
    let uid = seed_user(&pool, "alice", "tenant_admin", Some(t)).await;
    let token = issue_token(uid, "tenant_admin", Some(t), "alice");

    let resp = call(
        app(pool),
        json_request(
            "POST",
            "/api/knowledge",
            Some(&token),
            &json!({"filename": "runbook.md", "content": "# Ops"}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let v = body_json(resp).await;
    assert_eq!(v["tenant_id"], t.to_string());
    assert_eq!(v["created_by"], uid.to_string());
    assert!(v["size_bytes"].as_i64().unwrap() > 0);
}

#[sqlx::test(migrations = "src/migrations")]
async fn list_global_files_visible_across_tenants(pool: PgPool) {
    // Files with NULL account_id are "global docs" — visible to everyone.
    // Verify this documented contract.
    let t1 = seed_tenant(&pool, "t1").await;
    let t2 = seed_tenant(&pool, "t2").await;
    let u1 = seed_user(&pool, "u1", "tenant_admin", Some(t1)).await;
    let u2 = seed_user(&pool, "u2", "tenant_admin", Some(t2)).await;
    create_file(app(pool.clone()), &issue_token(u1, "tenant_admin", Some(t1), "u1"), "a.md").await;
    create_file(app(pool.clone()), &issue_token(u2, "tenant_admin", Some(t2), "u2"), "b.md").await;

    // t1 member (no explicit user seeded — purely read, no FK needed)
    let reader = issue_token(Uuid::new_v4(), "member", Some(t1), "reader");
    let resp = call(app(pool), http_get("/api/knowledge", Some(&reader))).await;
    assert_eq!(body_json(resp).await.as_array().unwrap().len(), 2);
}

#[sqlx::test(migrations = "src/migrations")]
async fn super_admin_sees_all(pool: PgPool) {
    let t = seed_tenant(&pool, "t1").await;
    let uid = seed_user(&pool, "author", "tenant_admin", Some(t)).await;
    let author = issue_token(uid, "tenant_admin", Some(t), "author");
    create_file(app(pool.clone()), &author, "a.md").await;
    create_file(app(pool.clone()), &author, "b.md").await;

    let admin = issue_token(Uuid::new_v4(), "super_admin", None, "admin");
    let resp = call(app(pool), http_get("/api/knowledge", Some(&admin))).await;
    assert_eq!(body_json(resp).await.as_array().unwrap().len(), 2);
}

#[sqlx::test(migrations = "src/migrations")]
async fn delete_own_succeeds(pool: PgPool) {
    let t = seed_tenant(&pool, "t1").await;
    let uid = seed_user(&pool, "u", "tenant_admin", Some(t)).await;
    let token = issue_token(uid, "tenant_admin", Some(t), "u");
    let id = create_file(app(pool.clone()), &token, "gone.md").await;
    let resp = call(app(pool), delete(&format!("/api/knowledge/{id}"), Some(&token))).await;
    assert_eq!(resp.status(), StatusCode::OK);
}
