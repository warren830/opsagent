//! HTTP-level tests for /api/account-access/* and /api/accounts/{id}/users,
//! plus /api/my/accessible-accounts. grant is covered in handler_rbac_test.rs.

mod helpers;

use axum::{
    Router,
    http::StatusCode,
    routing::{delete as del, get, post},
};
use helpers::http::{
    body_json, call, delete, get as http_get, issue_token, json_request, test_state, token_super_admin,
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
            .route(
                "/api/accounts/{id}/users",
                get(handlers::account_access::list_account_users),
            )
            .route("/api/account-access/grant", post(handlers::account_access::grant))
            .route(
                "/api/account-access/{user_id}/{account_id}",
                del(handlers::account_access::revoke),
            )
            .route(
                "/api/my/accessible-accounts",
                get(handlers::account_access::my_accessible_accounts),
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
    let v = body_json(resp).await;
    Uuid::parse_str(v["id"].as_str().unwrap()).unwrap()
}

async fn grant(app: Router, token: &str, uid: Uuid, aid: Uuid, role: &str) {
    let resp = call(
        app,
        json_request(
            "POST",
            "/api/account-access/grant",
            Some(token),
            &json!({"user_id": uid, "account_id": aid, "role": role}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
}

// ---- revoke ----

#[sqlx::test(migrations = "src/migrations")]
async fn revoke_by_super_admin_succeeds(pool: PgPool) {
    let tid = seed_tenant(&pool, "t1").await;
    let admin = token_super_admin();
    let acct = create_acct(app(pool.clone()), &admin, tid, "prod").await;
    let uid = seed_user(&pool, "alice", Some(tid)).await;
    grant(app(pool.clone()), &admin, uid, acct, "readonly").await;

    let resp = call(
        app(pool),
        delete(&format!("/api/account-access/{uid}/{acct}"), Some(&admin)),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[sqlx::test(migrations = "src/migrations")]
async fn revoke_nonexistent_grant_returns_not_found(pool: PgPool) {
    let admin = token_super_admin();
    let fake_uid = Uuid::new_v4();
    let fake_aid = Uuid::new_v4();
    let resp = call(
        app(pool),
        delete(&format!("/api/account-access/{fake_uid}/{fake_aid}"), Some(&admin)),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "src/migrations")]
async fn revoke_by_member_forbidden(pool: PgPool) {
    let tid = seed_tenant(&pool, "t1").await;
    let admin = token_super_admin();
    let acct = create_acct(app(pool.clone()), &admin, tid, "prod").await;
    let uid = seed_user(&pool, "alice", Some(tid)).await;
    grant(app(pool.clone()), &admin, uid, acct, "readonly").await;

    let attacker = issue_token(Uuid::new_v4(), "member", Some(tid), "attacker");
    let resp = call(
        app(pool),
        delete(&format!("/api/account-access/{uid}/{acct}"), Some(&attacker)),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "src/migrations")]
async fn revoke_cross_tenant_forbidden_for_tenant_admin(pool: PgPool) {
    let t1 = seed_tenant(&pool, "t1").await;
    let t2 = seed_tenant(&pool, "t2").await;
    let admin = token_super_admin();
    let acct_t2 = create_acct(app(pool.clone()), &admin, t2, "t2-prod").await;
    let uid = seed_user(&pool, "mallory", Some(t2)).await;
    grant(app(pool.clone()), &admin, uid, acct_t2, "readonly").await;

    let tadmin_t1 = token_tenant_admin(t1);
    let resp = call(
        app(pool),
        delete(&format!("/api/account-access/{uid}/{acct_t2}"), Some(&tadmin_t1)),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

// ---- list_account_users ----

#[sqlx::test(migrations = "src/migrations")]
async fn list_account_users_returns_granted_users(pool: PgPool) {
    let tid = seed_tenant(&pool, "t1").await;
    let admin = token_super_admin();
    let acct = create_acct(app(pool.clone()), &admin, tid, "prod").await;
    let u1 = seed_user(&pool, "alice", Some(tid)).await;
    let u2 = seed_user(&pool, "bob", Some(tid)).await;
    grant(app(pool.clone()), &admin, u1, acct, "readonly").await;
    grant(app(pool.clone()), &admin, u2, acct, "admin").await;

    let resp = call(app(pool), http_get(&format!("/api/accounts/{acct}/users"), Some(&admin))).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let v = body_json(resp).await;
    let arr = v.as_array().unwrap();
    assert_eq!(arr.len(), 2);
}

#[sqlx::test(migrations = "src/migrations")]
async fn list_account_users_forbidden_for_cross_tenant_admin(pool: PgPool) {
    let t1 = seed_tenant(&pool, "t1").await;
    let t2 = seed_tenant(&pool, "t2").await;
    let admin = token_super_admin();
    let acct_t2 = create_acct(app(pool.clone()), &admin, t2, "t2-prod").await;

    let tadmin_t1 = token_tenant_admin(t1);
    let resp = call(
        app(pool),
        http_get(&format!("/api/accounts/{acct_t2}/users"), Some(&tadmin_t1)),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "src/migrations")]
async fn list_account_users_not_found_for_missing_account(pool: PgPool) {
    let admin = token_super_admin();
    let ghost = Uuid::new_v4();
    let resp = call(app(pool), http_get(&format!("/api/accounts/{ghost}/users"), Some(&admin))).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

// ---- my/accessible-accounts ----

#[sqlx::test(migrations = "src/migrations")]
async fn my_accessible_accounts_reflects_grants_and_writable_flag(pool: PgPool) {
    let tid = seed_tenant(&pool, "t1").await;
    let admin = token_super_admin();
    let acct_ro = create_acct(app(pool.clone()), &admin, tid, "ro").await;
    let acct_rw = create_acct(app(pool.clone()), &admin, tid, "rw").await;
    let uid = seed_user(&pool, "alice", Some(tid)).await;
    grant(app(pool.clone()), &admin, uid, acct_ro, "readonly").await;
    grant(app(pool.clone()), &admin, uid, acct_rw, "admin").await;

    let alice = issue_token(uid, "member", Some(tid), "alice");
    let resp = call(app(pool), http_get("/api/my/accessible-accounts", Some(&alice))).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let v = body_json(resp).await;
    let arr = v.as_array().unwrap();
    assert_eq!(arr.len(), 2);

    let by_id: std::collections::HashMap<String, bool> = arr
        .iter()
        .map(|a| (a["id"].as_str().unwrap().to_string(), a["writable"].as_bool().unwrap()))
        .collect();
    assert_eq!(by_id[&acct_ro.to_string()], false, "readonly grant → writable=false");
    assert_eq!(by_id[&acct_rw.to_string()], true, "admin grant → writable=true");
}

#[sqlx::test(migrations = "src/migrations")]
async fn my_accessible_accounts_for_super_admin_all_writable(pool: PgPool) {
    let tid = seed_tenant(&pool, "t1").await;
    let admin = token_super_admin();
    create_acct(app(pool.clone()), &admin, tid, "a").await;
    create_acct(app(pool.clone()), &admin, tid, "b").await;

    let resp = call(app(pool), http_get("/api/my/accessible-accounts", Some(&admin))).await;
    let v = body_json(resp).await;
    let arr = v.as_array().unwrap();
    assert_eq!(arr.len(), 2);
    for a in arr {
        assert_eq!(a["writable"], true);
    }
}
