//! HTTP tests for /api/channels — CRUD + tenant isolation.

mod helpers;

use axum::{Router, http::StatusCode, routing::get};
use helpers::http::{
    body_json, call, delete, get as http_get, json_request, test_state, token_member, token_super_admin,
    token_tenant_admin, with_auth,
};
use ops::handlers;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

fn app(pool: PgPool) -> Router {
    with_auth(
        Router::new()
            .route(
                "/api/channels",
                get(handlers::channel::list).post(handlers::channel::create),
            )
            .route(
                "/api/channels/{id}",
                axum::routing::put(handlers::channel::update).delete(handlers::channel::delete),
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

async fn create_channel(app: Router, token: &str, name: &str) -> Uuid {
    let resp = call(
        app,
        json_request(
            "POST",
            "/api/channels",
            Some(token),
            &json!({"platform": "slack", "name": name}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    Uuid::parse_str(body_json(resp).await["id"].as_str().unwrap()).unwrap()
}

#[sqlx::test(migrations = "src/migrations")]
async fn create_assigns_callers_tenant(pool: PgPool) {
    let t = seed_tenant(&pool, "t1").await;
    let tadmin = token_tenant_admin(t);
    let resp = call(
        app(pool),
        json_request(
            "POST",
            "/api/channels",
            Some(&tadmin),
            &json!({"platform": "slack", "name": "#alerts"}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let v = body_json(resp).await;
    assert_eq!(v["tenant_id"], t.to_string());
    assert_eq!(v["platform"], "slack");
    assert_eq!(v["enabled"], true);
}

#[sqlx::test(migrations = "src/migrations")]
async fn list_scoped_by_tenant(pool: PgPool) {
    let t1 = seed_tenant(&pool, "t1").await;
    let t2 = seed_tenant(&pool, "t2").await;
    create_channel(app(pool.clone()), &token_tenant_admin(t1), "a").await;
    create_channel(app(pool.clone()), &token_tenant_admin(t2), "b").await;

    let m = token_member(t1);
    let resp = call(app(pool), http_get("/api/channels", Some(&m))).await;
    assert_eq!(body_json(resp).await.as_array().unwrap().len(), 1);
}

#[sqlx::test(migrations = "src/migrations")]
async fn super_admin_sees_all(pool: PgPool) {
    let t1 = seed_tenant(&pool, "t1").await;
    let t2 = seed_tenant(&pool, "t2").await;
    create_channel(app(pool.clone()), &token_tenant_admin(t1), "a").await;
    create_channel(app(pool.clone()), &token_tenant_admin(t2), "b").await;

    let admin = token_super_admin();
    let resp = call(app(pool), http_get("/api/channels", Some(&admin))).await;
    assert_eq!(body_json(resp).await.as_array().unwrap().len(), 2);
}

#[sqlx::test(migrations = "src/migrations")]
async fn update_cross_tenant_forbidden(pool: PgPool) {
    let t1 = seed_tenant(&pool, "t1").await;
    let t2 = seed_tenant(&pool, "t2").await;
    let id = create_channel(app(pool.clone()), &token_tenant_admin(t2), "other").await;

    let m = token_member(t1);
    let resp = call(
        app(pool),
        json_request(
            "PUT",
            &format!("/api/channels/{id}"),
            Some(&m),
            &json!({"name": "hijack"}),
        ),
    )
    .await;
    assert!(matches!(resp.status(), StatusCode::FORBIDDEN | StatusCode::NOT_FOUND));
}

#[sqlx::test(migrations = "src/migrations")]
async fn delete_own_succeeds(pool: PgPool) {
    let t = seed_tenant(&pool, "t1").await;
    let tadmin = token_tenant_admin(t);
    let id = create_channel(app(pool.clone()), &tadmin, "doomed").await;
    let resp = call(app(pool), delete(&format!("/api/channels/{id}"), Some(&tadmin))).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[sqlx::test(migrations = "src/migrations")]
async fn update_toggles_enabled_flag(pool: PgPool) {
    let t = seed_tenant(&pool, "t1").await;
    let tadmin = token_tenant_admin(t);
    let id = create_channel(app(pool.clone()), &tadmin, "toggle").await;
    let resp = call(
        app(pool),
        json_request(
            "PUT",
            &format!("/api/channels/{id}"),
            Some(&tadmin),
            &json!({"enabled": false}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(body_json(resp).await["enabled"], false);
}
