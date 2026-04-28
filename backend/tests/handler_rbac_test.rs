//! Cross-cutting RBAC matrix on /api/accounts routes.
//!
//! Validates CLAUDE.md's core access model:
//! 1. `readonly` grant → can list but NOT write (403 on update/delete)
//! 2. `admin` grant → full write (200 on update/delete)
//! 3. super_admin always writes
//! 4. tenant_admin can write to own-tenant accounts, not others

mod helpers;

use axum::{
    Router,
    http::StatusCode,
    routing::{get, post},
};
use helpers::http::{
    body_json, call, delete, issue_token, json_request, test_state, token_super_admin, token_tenant_admin, with_auth,
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
                "/api/accounts/{id}",
                axum::routing::put(handlers::cloud_account::update).delete(handlers::cloud_account::delete),
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

async fn seed_user(pool: &PgPool, username: &str, tenant_id: Option<Uuid>) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO users (username, password_hash, role, tenant_id) VALUES ($1, 'x', 'member', $2) RETURNING id",
    )
    .bind(username)
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn create_cloud_account(app: Router, token: &str, tenant_id: Uuid, name: &str) -> Uuid {
    let resp = call(
        app,
        json_request(
            "POST",
            "/api/accounts",
            Some(token),
            &json!({"provider": "aws", "name": name, "tenant_id": tenant_id, "is_mock": true}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK, "create_cloud_account failed");
    let v = body_json(resp).await;
    Uuid::parse_str(v["id"].as_str().unwrap()).unwrap()
}

async fn grant_access(app: Router, token: &str, user_id: Uuid, account_id: Uuid, role: &str) {
    let resp = call(
        app,
        json_request(
            "POST",
            "/api/account-access/grant",
            Some(token),
            &json!({"user_id": user_id, "account_id": account_id, "role": role}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK, "grant_access failed: {:?}", resp.status());
}

// ============================================================================
// 1. Grant matrix — readonly denies write, admin allows write
// ============================================================================

#[sqlx::test(migrations = "src/migrations")]
async fn readonly_grant_cannot_update_account(pool: PgPool) {
    let tid = seed_tenant(&pool, "t1").await;
    let admin = token_super_admin();
    let acct = create_cloud_account(app(pool.clone()), &admin, tid, "prod").await;
    let uid = seed_user(&pool, "alice", Some(tid)).await;
    grant_access(app(pool.clone()), &admin, uid, acct, "readonly").await;

    // Alice has a 'readonly' grant — update must be 403
    let alice_token = issue_token(uid, "member", Some(tid), "alice");
    let resp = call(
        app(pool),
        json_request(
            "PUT",
            &format!("/api/accounts/{acct}"),
            Some(&alice_token),
            &json!({"name": "hacked"}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "src/migrations")]
async fn readonly_grant_cannot_delete_account(pool: PgPool) {
    let tid = seed_tenant(&pool, "t1").await;
    let admin = token_super_admin();
    let acct = create_cloud_account(app(pool.clone()), &admin, tid, "prod").await;
    let uid = seed_user(&pool, "bob", Some(tid)).await;
    grant_access(app(pool.clone()), &admin, uid, acct, "readonly").await;

    let bob = issue_token(uid, "member", Some(tid), "bob");
    let resp = call(app(pool), delete(&format!("/api/accounts/{acct}"), Some(&bob))).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "src/migrations")]
async fn admin_grant_allows_update(pool: PgPool) {
    let tid = seed_tenant(&pool, "t1").await;
    let admin = token_super_admin();
    let acct = create_cloud_account(app(pool.clone()), &admin, tid, "prod").await;
    let uid = seed_user(&pool, "carol", Some(tid)).await;
    grant_access(app(pool.clone()), &admin, uid, acct, "admin").await;

    let carol = issue_token(uid, "member", Some(tid), "carol");
    let resp = call(
        app(pool),
        json_request(
            "PUT",
            &format!("/api/accounts/{acct}"),
            Some(&carol),
            &json!({"name": "renamed"}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(body_json(resp).await["name"], "renamed");
}

#[sqlx::test(migrations = "src/migrations")]
async fn no_grant_member_cannot_update(pool: PgPool) {
    let tid = seed_tenant(&pool, "t1").await;
    let admin = token_super_admin();
    let acct = create_cloud_account(app(pool.clone()), &admin, tid, "prod").await;
    let uid = seed_user(&pool, "dave", Some(tid)).await;
    // No grant issued for Dave

    let dave = issue_token(uid, "member", Some(tid), "dave");
    let resp = call(
        app(pool),
        json_request(
            "PUT",
            &format!("/api/accounts/{acct}"),
            Some(&dave),
            &json!({"name": "hacked"}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

// ============================================================================
// 2. Tenant isolation — tenant_admin cannot touch accounts in other tenants
// ============================================================================

#[sqlx::test(migrations = "src/migrations")]
async fn tenant_admin_cannot_update_cross_tenant_account(pool: PgPool) {
    let t1 = seed_tenant(&pool, "t1").await;
    let t2 = seed_tenant(&pool, "t2").await;
    let admin = token_super_admin();
    let acct_t2 = create_cloud_account(app(pool.clone()), &admin, t2, "t2-prod").await;

    // tenant_admin of t1 tries to update an account in t2
    let tadmin_t1 = token_tenant_admin(t1);
    let resp = call(
        app(pool),
        json_request(
            "PUT",
            &format!("/api/accounts/{acct_t2}"),
            Some(&tadmin_t1),
            &json!({"name": "hacked"}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "src/migrations")]
async fn tenant_admin_can_update_own_tenant_account(pool: PgPool) {
    let t1 = seed_tenant(&pool, "t1").await;
    let admin = token_super_admin();
    let acct = create_cloud_account(app(pool.clone()), &admin, t1, "t1-prod").await;

    let tadmin = token_tenant_admin(t1);
    let resp = call(
        app(pool),
        json_request(
            "PUT",
            &format!("/api/accounts/{acct}"),
            Some(&tadmin),
            &json!({"name": "renamed"}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
}

// ============================================================================
// 3. Grant endpoint — tenant_admin scope enforcement
// ============================================================================

#[sqlx::test(migrations = "src/migrations")]
async fn tenant_admin_cannot_grant_on_cross_tenant_account(pool: PgPool) {
    let t1 = seed_tenant(&pool, "t1").await;
    let t2 = seed_tenant(&pool, "t2").await;
    let admin = token_super_admin();
    let acct_t2 = create_cloud_account(app(pool.clone()), &admin, t2, "t2-prod").await;
    let uid = seed_user(&pool, "mallory", Some(t1)).await;

    let tadmin_t1 = token_tenant_admin(t1);
    let resp = call(
        app(pool),
        json_request(
            "POST",
            "/api/account-access/grant",
            Some(&tadmin_t1),
            &json!({"user_id": uid, "account_id": acct_t2, "role": "admin"}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "src/migrations")]
async fn member_cannot_grant_access(pool: PgPool) {
    let t1 = seed_tenant(&pool, "t1").await;
    let admin = token_super_admin();
    let acct = create_cloud_account(app(pool.clone()), &admin, t1, "prod").await;
    let victim = seed_user(&pool, "victim", Some(t1)).await;

    let member = issue_token(Uuid::new_v4(), "member", Some(t1), "attacker");
    let resp = call(
        app(pool),
        json_request(
            "POST",
            "/api/account-access/grant",
            Some(&member),
            &json!({"user_id": victim, "account_id": acct, "role": "admin"}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

// ============================================================================
// 4. Grant priority — explicit readonly overrides implicit tenant_admin write
// ============================================================================

#[sqlx::test(migrations = "src/migrations")]
async fn explicit_readonly_overrides_tenant_admin_write(pool: PgPool) {
    let tid = seed_tenant(&pool, "t1").await;
    let admin = token_super_admin();
    let acct = create_cloud_account(app(pool.clone()), &admin, tid, "prod").await;
    let uid = seed_user(&pool, "t_admin_user", Some(tid)).await;

    // Give the tenant_admin-role user an EXPLICIT readonly grant
    // Per CLAUDE.md: explicit grant takes priority over implicit tenant role
    grant_access(app(pool.clone()), &admin, uid, acct, "readonly").await;

    let constrained_tadmin = issue_token(uid, "tenant_admin", Some(tid), "t_admin_user");
    let resp = call(
        app(pool),
        json_request(
            "PUT",
            &format!("/api/accounts/{acct}"),
            Some(&constrained_tadmin),
            &json!({"name": "hacked"}),
        ),
    )
    .await;
    // Readonly grant must win — even though tenant_admin would normally write this tenant's account
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}
