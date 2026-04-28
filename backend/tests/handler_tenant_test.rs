//! HTTP-level tests for /api/tenants — CRUD + super_admin-only enforcement.

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
                "/api/tenants",
                get(handlers::tenant::list_tenants).post(handlers::tenant::create_tenant),
            )
            .route(
                "/api/tenants/{id}",
                get(handlers::tenant::get_tenant)
                    .put(handlers::tenant::update_tenant)
                    .delete(handlers::tenant::delete_tenant),
            ),
    )
    .with_state(test_state(pool))
}

async fn create_tenant(app: Router, token: &str, slug: &str) -> Uuid {
    let resp = call(
        app,
        json_request("POST", "/api/tenants", Some(token), &json!({"name": slug, "slug": slug})),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let v = body_json(resp).await;
    Uuid::parse_str(v["id"].as_str().unwrap()).unwrap()
}

#[sqlx::test(migrations = "src/migrations")]
async fn create_requires_super_admin(pool: PgPool) {
    let member_token = token_member(Uuid::new_v4());
    let resp = call(
        app(pool),
        json_request(
            "POST",
            "/api/tenants",
            Some(&member_token),
            &json!({"name": "x", "slug": "x"}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "src/migrations")]
async fn create_succeeds_as_super_admin(pool: PgPool) {
    let token = token_super_admin();
    let id = create_tenant(app(pool), &token, "acme").await;
    assert!(!id.is_nil());
}

#[sqlx::test(migrations = "src/migrations")]
async fn create_duplicate_slug_returns_conflict(pool: PgPool) {
    let token = token_super_admin();
    let a = app(pool.clone());
    create_tenant(a, &token, "dup").await;

    let resp = call(
        app(pool),
        json_request(
            "POST",
            "/api/tenants",
            Some(&token),
            &json!({"name": "other", "slug": "dup"}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CONFLICT);
}

#[sqlx::test(migrations = "src/migrations")]
async fn create_rejects_empty_name(pool: PgPool) {
    let token = token_super_admin();
    let resp = call(
        app(pool),
        json_request("POST", "/api/tenants", Some(&token), &json!({"name": "", "slug": "x"})),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "src/migrations")]
async fn list_as_member_returns_only_own_tenant(pool: PgPool) {
    let admin = token_super_admin();
    let t1 = create_tenant(app(pool.clone()), &admin, "one").await;
    create_tenant(app(pool.clone()), &admin, "two").await;

    let member = token_member(t1);
    let resp = call(app(pool), http_get("/api/tenants", Some(&member))).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let v = body_json(resp).await;
    let arr = v.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["id"], t1.to_string());
}

#[sqlx::test(migrations = "src/migrations")]
async fn get_cross_tenant_forbidden(pool: PgPool) {
    let admin = token_super_admin();
    let t1 = create_tenant(app(pool.clone()), &admin, "alpha").await;
    let t2 = create_tenant(app(pool.clone()), &admin, "beta").await;

    // Member of t1 tries to fetch t2 → 403
    let member_t1 = token_member(t1);
    let resp = call(app(pool), http_get(&format!("/api/tenants/{t2}"), Some(&member_t1))).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "src/migrations")]
async fn update_requires_super_admin(pool: PgPool) {
    let admin = token_super_admin();
    let t = create_tenant(app(pool.clone()), &admin, "upd").await;

    // member → 403
    let member = token_member(t);
    let resp = call(
        app(pool.clone()),
        json_request(
            "PUT",
            &format!("/api/tenants/{t}"),
            Some(&member),
            &json!({"name": "hacked"}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    // tenant_admin (even of same tenant) → 403 (tenant update is super_admin-only by design)
    let tadmin = token_tenant_admin(t);
    let resp = call(
        app(pool.clone()),
        json_request(
            "PUT",
            &format!("/api/tenants/{t}"),
            Some(&tadmin),
            &json!({"name": "hacked"}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    // super_admin → 200
    let resp = call(
        app(pool),
        json_request(
            "PUT",
            &format!("/api/tenants/{t}"),
            Some(&admin),
            &json!({"name": "renamed"}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[sqlx::test(migrations = "src/migrations")]
async fn delete_requires_super_admin(pool: PgPool) {
    let admin = token_super_admin();
    let t = create_tenant(app(pool.clone()), &admin, "del").await;

    // tenant_admin of same tenant still cannot delete
    let tadmin = token_tenant_admin(t);
    let resp = call(app(pool.clone()), delete(&format!("/api/tenants/{t}"), Some(&tadmin))).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    // super_admin can
    let resp = call(app(pool), delete(&format!("/api/tenants/{t}"), Some(&admin))).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[sqlx::test(migrations = "src/migrations")]
async fn update_nonexistent_tenant_returns_not_found(pool: PgPool) {
    let admin = token_super_admin();
    let ghost = Uuid::new_v4();
    let resp = call(
        app(pool),
        json_request(
            "PUT",
            &format!("/api/tenants/{ghost}"),
            Some(&admin),
            &json!({"name": "ghost"}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
