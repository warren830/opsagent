//! HTTP tests for /api/scheduled-jobs — CRUD + tenant scope.
//! trigger_run spawns background work (Claude subprocess) → not unit-tested here.

mod helpers;

use axum::{Router, http::StatusCode, routing::{get, put}};
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
                "/api/scheduled-jobs",
                get(handlers::scheduled_job::list).post(handlers::scheduled_job::create),
            )
            .route(
                "/api/scheduled-jobs/{id}",
                put(handlers::scheduled_job::update).delete(handlers::scheduled_job::delete),
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

async fn seed_user(pool: &PgPool, role: &str, tid: Option<Uuid>) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO users (username, password_hash, role, tenant_id) VALUES ($1, 'x', $2, $3) RETURNING id",
    )
    .bind(format!("u-{}", Uuid::new_v4()))
    .bind(role)
    .bind(tid)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn create_job(app: Router, token: &str, name: &str) -> Uuid {
    let resp = call(
        app,
        json_request(
            "POST",
            "/api/scheduled-jobs",
            Some(token),
            &json!({"name": name, "cron_expression": "0 * * * *"}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    Uuid::parse_str(body_json(resp).await["id"].as_str().unwrap()).unwrap()
}

#[sqlx::test(migrations = "src/migrations")]
async fn create_rejects_empty_name(pool: PgPool) {
    let t = seed_tenant(&pool, "t1").await;
    let uid = seed_user(&pool, "tenant_admin", Some(t)).await;
    let token = issue_token(uid, "tenant_admin", Some(t), "u");
    let resp = call(
        app(pool),
        json_request(
            "POST",
            "/api/scheduled-jobs",
            Some(&token),
            &json!({"name": "", "cron_expression": "* * * * *"}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "src/migrations")]
async fn create_rejects_empty_cron(pool: PgPool) {
    let t = seed_tenant(&pool, "t1").await;
    let uid = seed_user(&pool, "tenant_admin", Some(t)).await;
    let token = issue_token(uid, "tenant_admin", Some(t), "u");
    let resp = call(
        app(pool),
        json_request(
            "POST",
            "/api/scheduled-jobs",
            Some(&token),
            &json!({"name": "x", "cron_expression": ""}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "src/migrations")]
async fn skill_job_requires_skill_path(pool: PgPool) {
    let t = seed_tenant(&pool, "t1").await;
    let uid = seed_user(&pool, "tenant_admin", Some(t)).await;
    let token = issue_token(uid, "tenant_admin", Some(t), "u");
    let resp = call(
        app(pool),
        json_request(
            "POST",
            "/api/scheduled-jobs",
            Some(&token),
            &json!({"name": "x", "cron_expression": "* * * * *", "job_type": "skill"}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "src/migrations")]
async fn create_assigns_caller_tenant_and_user(pool: PgPool) {
    let t = seed_tenant(&pool, "t1").await;
    let uid = seed_user(&pool, "member", Some(t)).await;
    let token = issue_token(uid, "member", Some(t), "u");
    let resp = call(
        app(pool),
        json_request(
            "POST",
            "/api/scheduled-jobs",
            Some(&token),
            &json!({"name": "daily", "cron_expression": "0 9 * * *"}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let v = body_json(resp).await;
    assert_eq!(v["tenant_id"], t.to_string());
    assert_eq!(v["created_by"], uid.to_string());
    assert_eq!(v["visibility"], "public");
    assert_eq!(v["job_type"], "agent");
}

#[sqlx::test(migrations = "src/migrations")]
async fn private_job_binds_to_creator(pool: PgPool) {
    let t = seed_tenant(&pool, "t1").await;
    let uid = seed_user(&pool, "member", Some(t)).await;
    let token = issue_token(uid, "member", Some(t), "u");
    let resp = call(
        app(pool),
        json_request(
            "POST",
            "/api/scheduled-jobs",
            Some(&token),
            &json!({"name": "mine", "cron_expression": "* * * * *", "visibility": "private"}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(body_json(resp).await["user_id"], uid.to_string());
}

#[sqlx::test(migrations = "src/migrations")]
async fn list_scopes_by_tenant(pool: PgPool) {
    let t1 = seed_tenant(&pool, "t1").await;
    let t2 = seed_tenant(&pool, "t2").await;
    let u1 = seed_user(&pool, "tenant_admin", Some(t1)).await;
    let u2 = seed_user(&pool, "tenant_admin", Some(t2)).await;
    create_job(app(pool.clone()), &issue_token(u1, "tenant_admin", Some(t1), "u1"), "a").await;
    create_job(app(pool.clone()), &issue_token(u2, "tenant_admin", Some(t2), "u2"), "b").await;

    let reader = issue_token(Uuid::new_v4(), "member", Some(t1), "r");
    let resp = call(app(pool), http_get("/api/scheduled-jobs", Some(&reader))).await;
    let arr = body_json(resp).await;
    assert_eq!(arr.as_array().unwrap().len(), 1);
}

#[sqlx::test(migrations = "src/migrations")]
async fn update_cross_tenant_forbidden(pool: PgPool) {
    let t1 = seed_tenant(&pool, "t1").await;
    let t2 = seed_tenant(&pool, "t2").await;
    let u2 = seed_user(&pool, "tenant_admin", Some(t2)).await;
    let id = create_job(
        app(pool.clone()),
        &issue_token(u2, "tenant_admin", Some(t2), "u2"),
        "other",
    )
    .await;

    let attacker = issue_token(Uuid::new_v4(), "member", Some(t1), "attacker");
    let resp = call(
        app(pool),
        json_request(
            "PUT",
            &format!("/api/scheduled-jobs/{id}"),
            Some(&attacker),
            &json!({"name": "hijack"}),
        ),
    )
    .await;
    assert!(matches!(resp.status(), StatusCode::FORBIDDEN | StatusCode::NOT_FOUND));
}

#[sqlx::test(migrations = "src/migrations")]
async fn delete_own_job(pool: PgPool) {
    let t = seed_tenant(&pool, "t1").await;
    let uid = seed_user(&pool, "tenant_admin", Some(t)).await;
    let token = issue_token(uid, "tenant_admin", Some(t), "u");
    let id = create_job(app(pool.clone()), &token, "gone").await;
    let resp = call(app(pool), delete(&format!("/api/scheduled-jobs/{id}"), Some(&token))).await;
    assert_eq!(resp.status(), StatusCode::OK);
}
