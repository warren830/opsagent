//! HTTP tests for /api/providers — admin-only CRUD.

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
                "/api/providers",
                get(handlers::provider::list).post(handlers::provider::create),
            )
            .route(
                "/api/providers/{id}",
                axum::routing::put(handlers::provider::update).delete(handlers::provider::delete),
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

#[sqlx::test(migrations = "src/migrations")]
async fn create_requires_admin(pool: PgPool) {
    let t = seed_tenant(&pool, "t1").await;
    let member = token_member(t);
    let resp = call(
        app(pool),
        json_request(
            "POST",
            "/api/providers",
            Some(&member),
            &json!({"name": "p", "provider_type": "claude", "config": {}}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "src/migrations")]
async fn create_rejects_empty_name(pool: PgPool) {
    let t = seed_tenant(&pool, "t1").await;
    let tadmin = token_tenant_admin(t);
    let resp = call(
        app(pool),
        json_request(
            "POST",
            "/api/providers",
            Some(&tadmin),
            &json!({"name": "", "provider_type": "claude", "config": {}}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "src/migrations")]
async fn first_provider_is_forced_default(pool: PgPool) {
    let t = seed_tenant(&pool, "t1").await;
    let tadmin = token_tenant_admin(t);
    let resp = call(
        app(pool),
        json_request(
            "POST",
            "/api/providers",
            Some(&tadmin),
            &json!({"name": "primary", "provider_type": "claude", "config": {}, "is_default": false}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        body_json(resp).await["is_default"], true,
        "first provider must be forced to default regardless of request"
    );
}

#[sqlx::test(migrations = "src/migrations")]
async fn new_default_unsets_existing_default(pool: PgPool) {
    let t = seed_tenant(&pool, "t1").await;
    let tadmin = token_tenant_admin(t);

    let resp = call(
        app(pool.clone()),
        json_request(
            "POST",
            "/api/providers",
            Some(&tadmin),
            &json!({"name": "a", "provider_type": "claude", "config": {}}),
        ),
    )
    .await;
    let a_id = Uuid::parse_str(body_json(resp).await["id"].as_str().unwrap()).unwrap();

    // Second provider with is_default=true → first is demoted
    call(
        app(pool.clone()),
        json_request(
            "POST",
            "/api/providers",
            Some(&tadmin),
            &json!({"name": "b", "provider_type": "claude", "config": {}, "is_default": true}),
        ),
    )
    .await;

    let resp = call(app(pool), http_get("/api/providers", Some(&tadmin))).await;
    let arr = body_json(resp).await;
    let arr = arr.as_array().unwrap();
    assert_eq!(arr.len(), 2);
    let a = arr.iter().find(|p| p["id"] == a_id.to_string()).unwrap();
    assert_eq!(a["is_default"], false, "old default must be demoted");
}

#[sqlx::test(migrations = "src/migrations")]
async fn list_scoped_by_tenant(pool: PgPool) {
    let t1 = seed_tenant(&pool, "t1").await;
    let t2 = seed_tenant(&pool, "t2").await;
    call(
        app(pool.clone()),
        json_request(
            "POST",
            "/api/providers",
            Some(&token_tenant_admin(t1)),
            &json!({"name": "t1p", "provider_type": "claude", "config": {}}),
        ),
    )
    .await;
    call(
        app(pool.clone()),
        json_request(
            "POST",
            "/api/providers",
            Some(&token_tenant_admin(t2)),
            &json!({"name": "t2p", "provider_type": "claude", "config": {}}),
        ),
    )
    .await;

    let resp = call(app(pool), http_get("/api/providers", Some(&token_tenant_admin(t1)))).await;
    assert_eq!(body_json(resp).await.as_array().unwrap().len(), 1);
}

#[sqlx::test(migrations = "src/migrations")]
async fn delete_requires_admin(pool: PgPool) {
    let t = seed_tenant(&pool, "t1").await;
    let tadmin = token_tenant_admin(t);
    let resp = call(
        app(pool.clone()),
        json_request(
            "POST",
            "/api/providers",
            Some(&tadmin),
            &json!({"name": "victim", "provider_type": "claude", "config": {}}),
        ),
    )
    .await;
    let id = Uuid::parse_str(body_json(resp).await["id"].as_str().unwrap()).unwrap();

    let member = token_member(t);
    let resp = call(
        app(pool.clone()),
        delete(&format!("/api/providers/{id}"), Some(&member)),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    // super_admin can delete
    let resp = call(
        app(pool),
        delete(&format!("/api/providers/{id}"), Some(&token_super_admin())),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
}
