//! HTTP-level tests for /api/clusters — tenant ownership semantics.
//!
//! Note: cluster::create has no role check in the service layer; ANY authenticated
//! user can create a cluster, but it is scoped to their tenant_id. We verify that.

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
                "/api/clusters",
                get(handlers::cluster::list).post(handlers::cluster::create),
            )
            .route(
                "/api/clusters/{id}",
                axum::routing::put(handlers::cluster::update).delete(handlers::cluster::delete),
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

async fn create_cluster(app: Router, token: &str, name: &str) -> Uuid {
    let resp = call(
        app,
        json_request(
            "POST",
            "/api/clusters",
            Some(token),
            &json!({"name": name, "cloud": "aws", "cluster_type": "eks"}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK, "create cluster failed");
    let v = body_json(resp).await;
    Uuid::parse_str(v["id"].as_str().unwrap()).unwrap()
}

#[sqlx::test(migrations = "src/migrations")]
async fn create_rejects_empty_name(pool: PgPool) {
    let admin = token_super_admin();
    let resp = call(
        app(pool),
        json_request("POST", "/api/clusters", Some(&admin), &json!({"name": ""})),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "src/migrations")]
async fn create_assigns_cluster_to_callers_tenant(pool: PgPool) {
    let t1 = seed_tenant(&pool, "t1").await;
    let tadmin = token_tenant_admin(t1);
    let resp = call(
        app(pool),
        json_request(
            "POST",
            "/api/clusters",
            Some(&tadmin),
            &json!({"name": "my-eks", "cloud": "aws", "cluster_type": "eks"}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let v = body_json(resp).await;
    assert_eq!(v["tenant_id"], t1.to_string());
    assert_eq!(v["name"], "my-eks");
}

#[sqlx::test(migrations = "src/migrations")]
async fn list_scopes_by_tenant_for_non_super_admin(pool: PgPool) {
    let t1 = seed_tenant(&pool, "t1").await;
    let t2 = seed_tenant(&pool, "t2").await;

    let tadmin_t1 = token_tenant_admin(t1);
    let tadmin_t2 = token_tenant_admin(t2);
    create_cluster(app(pool.clone()), &tadmin_t1, "c1").await;
    create_cluster(app(pool.clone()), &tadmin_t2, "c2").await;

    // tenant_admin t1 sees 1
    let resp = call(app(pool.clone()), http_get("/api/clusters", Some(&tadmin_t1))).await;
    let arr = body_json(resp).await;
    assert_eq!(arr.as_array().unwrap().len(), 1);

    // super_admin sees 2
    let admin = token_super_admin();
    let resp = call(app(pool), http_get("/api/clusters", Some(&admin))).await;
    assert_eq!(body_json(resp).await.as_array().unwrap().len(), 2);
}

#[sqlx::test(migrations = "src/migrations")]
async fn update_cross_tenant_forbidden(pool: PgPool) {
    let t1 = seed_tenant(&pool, "t1").await;
    let t2 = seed_tenant(&pool, "t2").await;
    let tadmin_t2 = token_tenant_admin(t2);
    let cluster_t2 = create_cluster(app(pool.clone()), &tadmin_t2, "c2").await;

    // tenant_admin t1 tries to touch t2's cluster
    let tadmin_t1 = token_tenant_admin(t1);
    let resp = call(
        app(pool),
        json_request(
            "PUT",
            &format!("/api/clusters/{cluster_t2}"),
            Some(&tadmin_t1),
            &json!({"name": "hijacked"}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "src/migrations")]
async fn update_own_tenant_cluster_succeeds(pool: PgPool) {
    let t1 = seed_tenant(&pool, "t1").await;
    let tadmin = token_tenant_admin(t1);
    let c = create_cluster(app(pool.clone()), &tadmin, "original").await;

    let resp = call(
        app(pool),
        json_request(
            "PUT",
            &format!("/api/clusters/{c}"),
            Some(&tadmin),
            &json!({"name": "renamed"}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(body_json(resp).await["name"], "renamed");
}

#[sqlx::test(migrations = "src/migrations")]
async fn delete_cross_tenant_forbidden(pool: PgPool) {
    let t1 = seed_tenant(&pool, "t1").await;
    let t2 = seed_tenant(&pool, "t2").await;
    let tadmin_t2 = token_tenant_admin(t2);
    let c = create_cluster(app(pool.clone()), &tadmin_t2, "c").await;

    let member_t1 = token_member(t1);
    let resp = call(app(pool), delete(&format!("/api/clusters/{c}"), Some(&member_t1))).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "src/migrations")]
async fn update_nonexistent_cluster_not_found(pool: PgPool) {
    let admin = token_super_admin();
    let ghost = Uuid::new_v4();
    let resp = call(
        app(pool),
        json_request("PUT", &format!("/api/clusters/{ghost}"), Some(&admin), &json!({})),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "src/migrations")]
async fn delete_own_tenant_cluster_succeeds(pool: PgPool) {
    let t1 = seed_tenant(&pool, "t1").await;
    let tadmin = token_tenant_admin(t1);
    let c = create_cluster(app(pool.clone()), &tadmin, "victim").await;
    let resp = call(app(pool), delete(&format!("/api/clusters/{c}"), Some(&tadmin))).await;
    assert_eq!(resp.status(), StatusCode::OK);
}
