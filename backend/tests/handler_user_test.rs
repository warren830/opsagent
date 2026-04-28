//! HTTP-level tests for /api/users — CRUD + super_admin-only enforcement.

mod helpers;

use axum::{Router, http::StatusCode, routing::get};
use helpers::http::{
    body_json, call, delete, get as http_get, issue_token, json_request, test_state, token_member, token_super_admin,
    with_auth,
};
use ops::handlers;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

fn app(pool: PgPool) -> Router {
    with_auth(
        Router::new()
            .route(
                "/api/users",
                get(handlers::user::list_users).post(handlers::user::create_user),
            )
            .route(
                "/api/users/{id}",
                get(handlers::user::list_users)
                    .put(handlers::user::update_user)
                    .delete(handlers::user::delete_user),
            )
            .route("/api/users/invite", axum::routing::post(handlers::user::invite_user)),
    )
    .with_state(test_state(pool))
}

/// Seed a tenant directly via SQL (faster than going through the handler).
async fn seed_tenant(pool: &PgPool, slug: &str) -> Uuid {
    sqlx::query_scalar::<_, Uuid>("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind(slug)
        .bind(slug)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrations = "src/migrations")]
async fn create_requires_super_admin(pool: PgPool) {
    let t = seed_tenant(&pool, "t1").await;
    let member = token_member(t);
    let resp = call(
        app(pool),
        json_request(
            "POST",
            "/api/users",
            Some(&member),
            &json!({"username": "alice", "password": "password123", "role": "member", "tenant_id": t}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "src/migrations")]
async fn create_member_succeeds(pool: PgPool) {
    let t = seed_tenant(&pool, "t1").await;
    let admin = token_super_admin();
    let resp = call(
        app(pool),
        json_request(
            "POST",
            "/api/users",
            Some(&admin),
            &json!({"username": "alice", "password": "password123", "role": "member", "tenant_id": t}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    assert_eq!(body["username"], "alice");
    assert_eq!(body["role"], "member");
}

#[sqlx::test(migrations = "src/migrations")]
async fn create_member_without_tenant_rejected(pool: PgPool) {
    let admin = token_super_admin();
    let resp = call(
        app(pool),
        json_request(
            "POST",
            "/api/users",
            Some(&admin),
            &json!({"username": "bob", "password": "password123", "role": "member"}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "src/migrations")]
async fn create_short_password_rejected(pool: PgPool) {
    let admin = token_super_admin();
    let resp = call(
        app(pool),
        json_request(
            "POST",
            "/api/users",
            Some(&admin),
            &json!({"username": "bob", "password": "short", "role": "super_admin"}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "src/migrations")]
async fn create_invalid_role_rejected(pool: PgPool) {
    let admin = token_super_admin();
    let resp = call(
        app(pool),
        json_request(
            "POST",
            "/api/users",
            Some(&admin),
            &json!({"username": "bob", "password": "password123", "role": "tenant_admin"}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "src/migrations")]
async fn create_duplicate_username_conflict(pool: PgPool) {
    let t = seed_tenant(&pool, "t1").await;
    let admin = token_super_admin();
    let payload =
        json!({"username": "dup", "password": "password123", "role": "member", "tenant_id": t});

    let resp = call(app(pool.clone()), json_request("POST", "/api/users", Some(&admin), &payload)).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = call(app(pool), json_request("POST", "/api/users", Some(&admin), &payload)).await;
    assert_eq!(resp.status(), StatusCode::CONFLICT);
}

#[sqlx::test(migrations = "src/migrations")]
async fn list_as_member_scoped_to_tenant(pool: PgPool) {
    let t1 = seed_tenant(&pool, "t1").await;
    let t2 = seed_tenant(&pool, "t2").await;
    let admin = token_super_admin();

    for (slug, tid) in [("u1-a", t1), ("u1-b", t1), ("u2-a", t2)] {
        call(
            app(pool.clone()),
            json_request(
                "POST",
                "/api/users",
                Some(&admin),
                &json!({"username": slug, "password": "password123", "role": "member", "tenant_id": tid}),
            ),
        )
        .await;
    }

    let member = token_member(t1);
    let resp = call(app(pool), http_get("/api/users", Some(&member))).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let arr = body_json(resp).await;
    let arr = arr.as_array().unwrap();
    assert_eq!(arr.len(), 2, "member should see only own-tenant users");
    for u in arr {
        assert_eq!(u["tenant_id"], t1.to_string());
    }
}

#[sqlx::test(migrations = "src/migrations")]
async fn delete_self_forbidden(pool: PgPool) {
    let t = seed_tenant(&pool, "t1").await;
    let admin_token = token_super_admin();

    // Create a user via handler to get its id, then try to delete *that* user
    // using a token whose `sub` matches (simulating self-delete).
    let create_resp = call(
        app(pool.clone()),
        json_request(
            "POST",
            "/api/users",
            Some(&admin_token),
            &json!({"username": "sam", "password": "password123", "role": "member", "tenant_id": t}),
        ),
    )
    .await;
    let created = body_json(create_resp).await;
    let sam_id = Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();

    // Issue a super_admin token whose `sub == sam_id`
    let self_token = issue_token(sam_id, "super_admin", None, "sam");
    let resp = call(app(pool), delete(&format!("/api/users/{sam_id}"), Some(&self_token))).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "src/migrations")]
async fn delete_nonexistent_user_not_found(pool: PgPool) {
    let admin = token_super_admin();
    let ghost = Uuid::new_v4();
    let resp = call(app(pool), delete(&format!("/api/users/{ghost}"), Some(&admin))).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "src/migrations")]
async fn invite_rejects_bad_email(pool: PgPool) {
    let admin = token_super_admin();
    let resp = call(
        app(pool),
        json_request(
            "POST",
            "/api/users/invite",
            Some(&admin),
            &json!({"email": "not-an-email"}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "src/migrations")]
async fn invite_requires_super_admin(pool: PgPool) {
    let t = seed_tenant(&pool, "t1").await;
    let member = token_member(t);
    let resp = call(
        app(pool),
        json_request(
            "POST",
            "/api/users/invite",
            Some(&member),
            &json!({"email": "new@example.com"}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}
