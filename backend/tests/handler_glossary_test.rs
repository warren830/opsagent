//! HTTP tests for /api/glossary — CRUD + tenant isolation.

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
                "/api/glossary",
                get(handlers::glossary::list).post(handlers::glossary::create),
            )
            .route(
                "/api/glossary/{id}",
                axum::routing::put(handlers::glossary::update).delete(handlers::glossary::delete),
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

async fn create_entry(app: Router, token: &str, term: &str) -> Uuid {
    let resp = call(
        app,
        json_request("POST", "/api/glossary", Some(token), &json!({"term": term})),
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
            "/api/glossary",
            Some(&tadmin),
            &json!({"term": "MTTR", "full_name": "Mean Time To Recovery"}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let v = body_json(resp).await;
    assert_eq!(v["tenant_id"], t.to_string());
    assert_eq!(v["term"], "MTTR");
}

#[sqlx::test(migrations = "src/migrations")]
async fn list_global_entries_visible_across_tenants(pool: PgPool) {
    // Entries without account_id are intentionally global (shared terminology).
    // Verify this is the documented behaviour, not a leak.
    let t1 = seed_tenant(&pool, "t1").await;
    let t2 = seed_tenant(&pool, "t2").await;
    create_entry(app(pool.clone()), &token_tenant_admin(t1), "GLOBAL-A").await;
    create_entry(app(pool.clone()), &token_tenant_admin(t2), "GLOBAL-B").await;

    let m = token_member(t1);
    let resp = call(app(pool), http_get("/api/glossary", Some(&m))).await;
    let arr = body_json(resp).await;
    assert_eq!(
        arr.as_array().unwrap().len(),
        2,
        "global glossary entries (no account_id) should be visible to all tenants"
    );
}

#[sqlx::test(migrations = "src/migrations")]
async fn list_filters_by_query(pool: PgPool) {
    let t = seed_tenant(&pool, "t1").await;
    let tadmin = token_tenant_admin(t);
    create_entry(app(pool.clone()), &tadmin, "SLO").await;
    create_entry(app(pool.clone()), &tadmin, "SLA").await;
    create_entry(app(pool.clone()), &tadmin, "MTTR").await;

    let resp = call(app(pool), http_get("/api/glossary?q=SL", Some(&tadmin))).await;
    let arr = body_json(resp).await;
    assert_eq!(arr.as_array().unwrap().len(), 2);
}

#[sqlx::test(migrations = "src/migrations")]
async fn update_cross_tenant_forbidden(pool: PgPool) {
    let t1 = seed_tenant(&pool, "t1").await;
    let t2 = seed_tenant(&pool, "t2").await;
    let id = create_entry(app(pool.clone()), &token_tenant_admin(t2), "OTHER").await;

    let m_t1 = token_member(t1);
    let resp = call(
        app(pool),
        json_request(
            "PUT",
            &format!("/api/glossary/{id}"),
            Some(&m_t1),
            &json!({"term": "hijack"}),
        ),
    )
    .await;
    assert!(
        matches!(resp.status(), StatusCode::FORBIDDEN | StatusCode::NOT_FOUND),
        "expected 403/404, got {:?}",
        resp.status()
    );
}

#[sqlx::test(migrations = "src/migrations")]
async fn delete_own_tenant_entry_succeeds(pool: PgPool) {
    let t = seed_tenant(&pool, "t1").await;
    let tadmin = token_tenant_admin(t);
    let id = create_entry(app(pool.clone()), &tadmin, "DOOMED").await;
    let resp = call(app(pool), delete(&format!("/api/glossary/{id}"), Some(&tadmin))).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[sqlx::test(migrations = "src/migrations")]
async fn super_admin_sees_all(pool: PgPool) {
    let t1 = seed_tenant(&pool, "t1").await;
    let t2 = seed_tenant(&pool, "t2").await;
    create_entry(app(pool.clone()), &token_tenant_admin(t1), "X").await;
    create_entry(app(pool.clone()), &token_tenant_admin(t2), "Y").await;

    let admin = token_super_admin();
    let resp = call(app(pool), http_get("/api/glossary", Some(&admin))).await;
    let arr = body_json(resp).await;
    assert_eq!(arr.as_array().unwrap().len(), 2);
}
