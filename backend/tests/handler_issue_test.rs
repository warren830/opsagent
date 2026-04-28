//! HTTP-level tests for /api/issues — list, get, update, count.
//! RCA streaming endpoints require Claude subprocess and are exercised via E2E.

mod helpers;

use axum::{
    Router,
    http::StatusCode,
    routing::{get, put},
};
use helpers::http::{
    body_json, call, get as http_get, json_request, test_state, token_member, token_super_admin, with_auth,
};
use ops::handlers;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

fn app(pool: PgPool) -> Router {
    with_auth(
        Router::new()
            .route("/api/issues", get(handlers::issue::list))
            .route("/api/issues/count", get(handlers::issue::count))
            .route(
                "/api/issues/{id}",
                get(handlers::issue::get).route_layer(axum::middleware::from_fn(|r, n: axum::middleware::Next| async move { n.run(r).await })),
            )
            .route("/api/issues/{id}", put(handlers::issue::update)),
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

async fn seed_issue(pool: &PgPool, title: &str, tid: Option<Uuid>, severity: &str, status: &str) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO issues (title, severity, status, tenant_id) VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(title)
    .bind(severity)
    .bind(status)
    .bind(tid)
    .fetch_one(pool)
    .await
    .unwrap()
}

// ---- list ----

#[sqlx::test(migrations = "src/migrations")]
async fn list_scoped_by_tenant_for_member(pool: PgPool) {
    let t1 = seed_tenant(&pool, "t1").await;
    let t2 = seed_tenant(&pool, "t2").await;
    seed_issue(&pool, "own-a", Some(t1), "high", "open").await;
    seed_issue(&pool, "own-b", Some(t1), "medium", "open").await;
    seed_issue(&pool, "other", Some(t2), "high", "open").await;

    let m = token_member(t1);
    let resp = call(app(pool), http_get("/api/issues", Some(&m))).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let arr = body_json(resp).await;
    assert_eq!(arr.as_array().unwrap().len(), 2);
}

#[sqlx::test(migrations = "src/migrations")]
async fn list_super_admin_sees_all(pool: PgPool) {
    let t1 = seed_tenant(&pool, "t1").await;
    let t2 = seed_tenant(&pool, "t2").await;
    seed_issue(&pool, "a", Some(t1), "high", "open").await;
    seed_issue(&pool, "b", Some(t2), "high", "open").await;

    let admin = token_super_admin();
    let resp = call(app(pool), http_get("/api/issues", Some(&admin))).await;
    assert_eq!(body_json(resp).await.as_array().unwrap().len(), 2);
}

#[sqlx::test(migrations = "src/migrations")]
async fn list_filters_by_severity(pool: PgPool) {
    let t1 = seed_tenant(&pool, "t1").await;
    seed_issue(&pool, "a", Some(t1), "high", "open").await;
    seed_issue(&pool, "b", Some(t1), "low", "open").await;

    let admin = token_super_admin();
    let resp = call(app(pool), http_get("/api/issues?severity=high", Some(&admin))).await;
    let arr = body_json(resp).await;
    let arr = arr.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["severity"], "high");
}

#[sqlx::test(migrations = "src/migrations")]
async fn list_filters_by_status(pool: PgPool) {
    let t1 = seed_tenant(&pool, "t1").await;
    seed_issue(&pool, "x", Some(t1), "high", "open").await;
    seed_issue(&pool, "y", Some(t1), "high", "resolved").await;

    let admin = token_super_admin();
    let resp = call(app(pool), http_get("/api/issues?status=resolved", Some(&admin))).await;
    let arr = body_json(resp).await;
    assert_eq!(arr.as_array().unwrap().len(), 1);
}

// ---- count ----

#[sqlx::test(migrations = "src/migrations")]
async fn count_excludes_resolved(pool: PgPool) {
    let t1 = seed_tenant(&pool, "t1").await;
    seed_issue(&pool, "o1", Some(t1), "high", "open").await;
    seed_issue(&pool, "o2", Some(t1), "high", "acknowledged").await;
    seed_issue(&pool, "r", Some(t1), "high", "resolved").await;

    let admin = token_super_admin();
    let resp = call(app(pool), http_get("/api/issues/count", Some(&admin))).await;
    let v = body_json(resp).await;
    assert_eq!(v["count"], 2);
}

#[sqlx::test(migrations = "src/migrations")]
async fn count_scoped_by_tenant(pool: PgPool) {
    let t1 = seed_tenant(&pool, "t1").await;
    let t2 = seed_tenant(&pool, "t2").await;
    seed_issue(&pool, "t1-a", Some(t1), "high", "open").await;
    seed_issue(&pool, "t2-a", Some(t2), "high", "open").await;
    seed_issue(&pool, "t2-b", Some(t2), "high", "open").await;

    let m = token_member(t2);
    let resp = call(app(pool), http_get("/api/issues/count", Some(&m))).await;
    assert_eq!(body_json(resp).await["count"], 2);
}

// ---- get ----

#[sqlx::test(migrations = "src/migrations")]
async fn get_own_tenant_issue(pool: PgPool) {
    let t1 = seed_tenant(&pool, "t1").await;
    let i = seed_issue(&pool, "mine", Some(t1), "high", "open").await;
    let m = token_member(t1);
    let resp = call(app(pool), http_get(&format!("/api/issues/{i}"), Some(&m))).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(body_json(resp).await["title"], "mine");
}

#[sqlx::test(migrations = "src/migrations")]
async fn get_cross_tenant_forbidden(pool: PgPool) {
    let t1 = seed_tenant(&pool, "t1").await;
    let t2 = seed_tenant(&pool, "t2").await;
    let i = seed_issue(&pool, "other", Some(t2), "high", "open").await;

    let m = token_member(t1);
    let resp = call(app(pool), http_get(&format!("/api/issues/{i}"), Some(&m))).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "src/migrations")]
async fn get_nonexistent_issue_not_found(pool: PgPool) {
    let admin = token_super_admin();
    let ghost = Uuid::new_v4();
    let resp = call(app(pool), http_get(&format!("/api/issues/{ghost}"), Some(&admin))).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

// ---- update ----

#[sqlx::test(migrations = "src/migrations")]
async fn update_own_tenant_issue(pool: PgPool) {
    let t1 = seed_tenant(&pool, "t1").await;
    let i = seed_issue(&pool, "old", Some(t1), "high", "open").await;

    let m = token_member(t1);
    let resp = call(
        app(pool),
        json_request(
            "PUT",
            &format!("/api/issues/{i}"),
            Some(&m),
            &json!({"status": "acknowledged"}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(body_json(resp).await["status"], "acknowledged");
}

#[sqlx::test(migrations = "src/migrations")]
async fn update_cross_tenant_forbidden(pool: PgPool) {
    let t1 = seed_tenant(&pool, "t1").await;
    let t2 = seed_tenant(&pool, "t2").await;
    let i = seed_issue(&pool, "other", Some(t2), "high", "open").await;

    let m = token_member(t1);
    let resp = call(
        app(pool),
        json_request(
            "PUT",
            &format!("/api/issues/{i}"),
            Some(&m),
            &json!({"status": "resolved"}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}
