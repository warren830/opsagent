//! HTTP tests for /api/approvals — approve/reject with tenant isolation.

mod helpers;

use axum::{Router, http::StatusCode, routing::{get, post}};
use helpers::http::{
    body_json, call, get as http_get, issue_token, json_request, test_state, with_auth,
};
use ops::handlers;
use sqlx::PgPool;
use uuid::Uuid;

fn app(pool: PgPool) -> Router {
    with_auth(
        Router::new()
            .route("/api/approvals", get(handlers::approval::list))
            .route("/api/approvals/{id}/approve", post(handlers::approval::approve))
            .route("/api/approvals/{id}/reject", post(handlers::approval::reject)),
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

async fn seed_user(pool: &PgPool, name: &str, role: &str, tid: Option<Uuid>) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO users (username, password_hash, role, tenant_id) VALUES ($1, 'x', $2, $3) RETURNING id",
    )
    .bind(format!("{name}-{}", Uuid::new_v4()))
    .bind(role)
    .bind(tid)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn seed_approval(pool: &PgPool, tid: Option<Uuid>) -> Uuid {
    let requester = seed_user(pool, "req", "member", tid).await;
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO approvals (command, requested_by, tenant_id, status) VALUES ('kubectl delete', $1, $2, 'pending') RETURNING id",
    )
    .bind(requester)
    .bind(tid)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Returns (token, user_id) for a tenant_admin with an actual users row (so reviewed_by FK is satisfied).
async fn real_tenant_admin(pool: &PgPool, tid: Uuid) -> String {
    let uid = seed_user(pool, "tadmin", "tenant_admin", Some(tid)).await;
    issue_token(uid, "tenant_admin", Some(tid), "tadmin")
}

async fn real_super_admin(pool: &PgPool) -> String {
    let uid = seed_user(pool, "super", "super_admin", None).await;
    issue_token(uid, "super_admin", None, "super")
}

#[sqlx::test(migrations = "src/migrations")]
async fn list_scopes_by_tenant(pool: PgPool) {
    let t1 = seed_tenant(&pool, "t1").await;
    let t2 = seed_tenant(&pool, "t2").await;
    seed_approval(&pool, Some(t1)).await;
    seed_approval(&pool, Some(t1)).await;
    seed_approval(&pool, Some(t2)).await;

    let tadmin = real_tenant_admin(&pool, t1).await;
    let resp = call(app(pool), http_get("/api/approvals", Some(&tadmin))).await;
    assert_eq!(body_json(resp).await.as_array().unwrap().len(), 2);
}

#[sqlx::test(migrations = "src/migrations")]
async fn list_filters_by_status(pool: PgPool) {
    let t = seed_tenant(&pool, "t1").await;
    seed_approval(&pool, Some(t)).await;
    let aid = seed_approval(&pool, Some(t)).await;

    let admin = real_super_admin(&pool).await;
    call(
        app(pool.clone()),
        json_request::<serde_json::Value>("POST", &format!("/api/approvals/{aid}/approve"), Some(&admin), &serde_json::Value::Null),
    )
    .await;

    let tadmin = real_tenant_admin(&pool, t).await;
    let resp = call(app(pool), http_get("/api/approvals?status=pending", Some(&tadmin))).await;
    let arr = body_json(resp).await;
    assert_eq!(arr.as_array().unwrap().len(), 1);
    assert_eq!(arr[0]["status"], "pending");
}

#[sqlx::test(migrations = "src/migrations")]
async fn approve_succeeds_for_own_tenant(pool: PgPool) {
    let t = seed_tenant(&pool, "t1").await;
    let aid = seed_approval(&pool, Some(t)).await;
    let tadmin = real_tenant_admin(&pool, t).await;
    let resp = call(
        app(pool),
        json_request::<serde_json::Value>("POST", &format!("/api/approvals/{aid}/approve"), Some(&tadmin), &serde_json::Value::Null),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(body_json(resp).await["status"], "approved");
}

#[sqlx::test(migrations = "src/migrations")]
async fn approve_cross_tenant_forbidden(pool: PgPool) {
    let t1 = seed_tenant(&pool, "t1").await;
    let t2 = seed_tenant(&pool, "t2").await;
    let aid = seed_approval(&pool, Some(t2)).await;

    let tadmin_t1 = real_tenant_admin(&pool, t1).await;
    let resp = call(
        app(pool),
        json_request::<serde_json::Value>("POST", &format!("/api/approvals/{aid}/approve"), Some(&tadmin_t1), &serde_json::Value::Null),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "src/migrations")]
async fn approve_already_processed_returns_bad_request(pool: PgPool) {
    let t = seed_tenant(&pool, "t1").await;
    let aid = seed_approval(&pool, Some(t)).await;
    let tadmin = real_tenant_admin(&pool, t).await;

    let first = call(
        app(pool.clone()),
        json_request::<serde_json::Value>("POST", &format!("/api/approvals/{aid}/approve"), Some(&tadmin), &serde_json::Value::Null),
    )
    .await;
    assert_eq!(first.status(), StatusCode::OK);

    let resp = call(
        app(pool),
        json_request::<serde_json::Value>("POST", &format!("/api/approvals/{aid}/approve"), Some(&tadmin), &serde_json::Value::Null),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "src/migrations")]
async fn reject_succeeds(pool: PgPool) {
    let t = seed_tenant(&pool, "t1").await;
    let aid = seed_approval(&pool, Some(t)).await;
    let tadmin = real_tenant_admin(&pool, t).await;
    let resp = call(
        app(pool),
        json_request::<serde_json::Value>("POST", &format!("/api/approvals/{aid}/reject"), Some(&tadmin), &serde_json::Value::Null),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(body_json(resp).await["status"], "rejected");
}

#[sqlx::test(migrations = "src/migrations")]
async fn reject_nonexistent_returns_not_found(pool: PgPool) {
    let admin = real_super_admin(&pool).await;
    let ghost = Uuid::new_v4();
    let resp = call(
        app(pool),
        json_request::<serde_json::Value>("POST", &format!("/api/approvals/{ghost}/reject"), Some(&admin), &serde_json::Value::Null),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "src/migrations")]
async fn super_admin_can_approve_any_tenant(pool: PgPool) {
    let t2 = seed_tenant(&pool, "t2").await;
    let aid = seed_approval(&pool, Some(t2)).await;
    let admin = real_super_admin(&pool).await;
    let resp = call(
        app(pool),
        json_request::<serde_json::Value>("POST", &format!("/api/approvals/{aid}/approve"), Some(&admin), &serde_json::Value::Null),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
}
