//! HTTP-level tests for /api/accounts list scoping + create rules.
//! update/delete RBAC is covered in handler_rbac_test.rs.

mod helpers;

use axum::{
    Router,
    http::StatusCode,
    routing::{get, post},
};
use helpers::http::{
    body_json, call, get as http_get, issue_token, json_request, test_state, token_member, token_super_admin,
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
                "/api/accounts",
                get(handlers::cloud_account::list).post(handlers::cloud_account::create),
            )
            .route("/api/account-access/grant", post(handlers::account_access::grant)),
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

async fn seed_user(pool: &PgPool, name: &str, tid: Option<Uuid>) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO users (username, password_hash, role, tenant_id) VALUES ($1, 'x', 'member', $2) RETURNING id",
    )
    .bind(name)
    .bind(tid)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn create_acct(app: Router, token: &str, tid: Uuid, name: &str) -> Uuid {
    let resp = call(
        app,
        json_request(
            "POST",
            "/api/accounts",
            Some(token),
            &json!({"provider": "aws", "name": name, "tenant_id": tid, "is_mock": true}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK, "create failed");
    let v = body_json(resp).await;
    Uuid::parse_str(v["id"].as_str().unwrap()).unwrap()
}

// ---- create ----

#[sqlx::test(migrations = "src/migrations")]
async fn create_requires_admin(pool: PgPool) {
    let t = seed_tenant(&pool, "t1").await;
    let member = token_member(t);
    let resp = call(
        app(pool),
        json_request(
            "POST",
            "/api/accounts",
            Some(&member),
            &json!({"provider": "aws", "name": "x", "tenant_id": t}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "src/migrations")]
async fn create_rejects_empty_name(pool: PgPool) {
    let t = seed_tenant(&pool, "t1").await;
    let admin = token_super_admin();
    let resp = call(
        app(pool),
        json_request(
            "POST",
            "/api/accounts",
            Some(&admin),
            &json!({"provider": "aws", "name": "", "tenant_id": t}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "src/migrations")]
async fn create_rejects_empty_provider(pool: PgPool) {
    let t = seed_tenant(&pool, "t1").await;
    let admin = token_super_admin();
    let resp = call(
        app(pool),
        json_request(
            "POST",
            "/api/accounts",
            Some(&admin),
            &json!({"provider": "", "name": "x", "tenant_id": t}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "src/migrations")]
async fn tenant_admin_create_forces_own_tenant(pool: PgPool) {
    // tenant_admin passing a *different* tenant_id in the request
    // should end up assigning the account to the caller's tenant (service-level override).
    let t1 = seed_tenant(&pool, "t1").await;
    let t2 = seed_tenant(&pool, "t2").await;
    let tadmin_t1 = token_tenant_admin(t1);

    let resp = call(
        app(pool.clone()),
        json_request(
            "POST",
            "/api/accounts",
            Some(&tadmin_t1),
            &json!({"provider": "aws", "name": "sneaky", "tenant_id": t2, "is_mock": true}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    assert_eq!(
        body["tenant_id"], t1.to_string(),
        "tenant_admin's account must be assigned to own tenant, not the one they claimed"
    );
}

// ---- list scoping ----

#[sqlx::test(migrations = "src/migrations")]
async fn list_as_super_admin_sees_all(pool: PgPool) {
    let t1 = seed_tenant(&pool, "t1").await;
    let t2 = seed_tenant(&pool, "t2").await;
    let admin = token_super_admin();
    create_acct(app(pool.clone()), &admin, t1, "a1").await;
    create_acct(app(pool.clone()), &admin, t2, "a2").await;

    let resp = call(app(pool), http_get("/api/accounts", Some(&admin))).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(body_json(resp).await.as_array().unwrap().len(), 2);
}

#[sqlx::test(migrations = "src/migrations")]
async fn list_as_tenant_admin_sees_own_tenant_only(pool: PgPool) {
    let t1 = seed_tenant(&pool, "t1").await;
    let t2 = seed_tenant(&pool, "t2").await;
    let admin = token_super_admin();
    create_acct(app(pool.clone()), &admin, t1, "own").await;
    create_acct(app(pool.clone()), &admin, t2, "other").await;

    let tadmin = token_tenant_admin(t1);
    let resp = call(app(pool), http_get("/api/accounts", Some(&tadmin))).await;
    let arr = body_json(resp).await;
    let arr = arr.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["name"], "own");
}

#[sqlx::test(migrations = "src/migrations")]
async fn list_as_member_without_grants_is_empty(pool: PgPool) {
    let t1 = seed_tenant(&pool, "t1").await;
    let admin = token_super_admin();
    create_acct(app(pool.clone()), &admin, t1, "secret").await;

    let member = token_member(t1);
    let resp = call(app(pool), http_get("/api/accounts", Some(&member))).await;
    let arr = body_json(resp).await;
    assert_eq!(arr.as_array().unwrap().len(), 0);
}

#[sqlx::test(migrations = "src/migrations")]
async fn list_as_member_with_grant_sees_granted_account(pool: PgPool) {
    let t1 = seed_tenant(&pool, "t1").await;
    let admin = token_super_admin();
    let acct = create_acct(app(pool.clone()), &admin, t1, "granted").await;
    let uid = seed_user(&pool, "alice", Some(t1)).await;

    // grant via the handler
    call(
        app(pool.clone()),
        json_request(
            "POST",
            "/api/account-access/grant",
            Some(&admin),
            &json!({"user_id": uid, "account_id": acct, "role": "readonly"}),
        ),
    )
    .await;

    let alice = issue_token(uid, "member", Some(t1), "alice");
    let resp = call(app(pool), http_get("/api/accounts", Some(&alice))).await;
    let arr = body_json(resp).await;
    let arr = arr.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["id"], acct.to_string());
}
