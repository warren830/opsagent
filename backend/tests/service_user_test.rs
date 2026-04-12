mod helpers;

use sqlx::PgPool;
use uuid::Uuid;

use openops::error::AppError;
use openops::middleware::auth::AuthUser;
use openops::models::user::{CreateUserRequest, InviteUserRequest};
use openops::services::user;

// ── Auth helpers ────────────────────────────────────────────────────

fn super_admin() -> AuthUser {
    AuthUser {
        user_id: Uuid::new_v4(),
        role: "super_admin".to_string(),
        tenant_id: None,
        username: "test_admin".to_string(),
    }
}

fn member(tenant_id: Uuid) -> AuthUser {
    AuthUser {
        user_id: Uuid::new_v4(),
        role: "member".to_string(),
        tenant_id: Some(tenant_id),
        username: "test_member".to_string(),
    }
}

// ── Seed helpers ────────────────────────────────────────────────────

async fn seed_tenant(pool: &PgPool) -> Uuid {
    sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ('t1', 't1') RETURNING id")
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed_tenant_named(pool: &PgPool, name: &str, slug: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind(name)
        .bind(slug)
        .fetch_one(pool)
        .await
        .unwrap()
}

// ── Tests: list ─────────────────────────────────────────────────────

#[sqlx::test(migrations = "src/migrations")]
async fn test_list_super_admin_sees_all(pool: PgPool) {
    let admin = super_admin();
    let t1 = seed_tenant_named(&pool, "Tenant A", "ta").await;
    let t2 = seed_tenant_named(&pool, "Tenant B", "tb").await;

    // Create one user in each tenant
    user::create(
        &pool,
        &admin,
        CreateUserRequest {
            username: "user_t1".to_string(),
            password: "password1234".to_string(),
            role: "member".to_string(),
            tenant_id: Some(t1),
            email: None,
        },
    )
    .await
    .unwrap();

    user::create(
        &pool,
        &admin,
        CreateUserRequest {
            username: "user_t2".to_string(),
            password: "password1234".to_string(),
            role: "member".to_string(),
            tenant_id: Some(t2),
            email: None,
        },
    )
    .await
    .unwrap();

    let users = user::list(&pool, &admin).await.unwrap();
    assert!(users.len() >= 2, "super_admin should see all users");

    let usernames: Vec<&str> = users.iter().map(|u| u.username.as_str()).collect();
    assert!(usernames.contains(&"user_t1"));
    assert!(usernames.contains(&"user_t2"));
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_list_member_sees_own_tenant(pool: PgPool) {
    let admin = super_admin();
    let t1 = seed_tenant_named(&pool, "Tenant A", "ta").await;
    let t2 = seed_tenant_named(&pool, "Tenant B", "tb").await;

    user::create(
        &pool,
        &admin,
        CreateUserRequest {
            username: "user_t1".to_string(),
            password: "password1234".to_string(),
            role: "member".to_string(),
            tenant_id: Some(t1),
            email: None,
        },
    )
    .await
    .unwrap();

    user::create(
        &pool,
        &admin,
        CreateUserRequest {
            username: "user_t2".to_string(),
            password: "password1234".to_string(),
            role: "member".to_string(),
            tenant_id: Some(t2),
            email: None,
        },
    )
    .await
    .unwrap();

    let m = member(t1);
    let users = user::list(&pool, &m).await.unwrap();
    assert_eq!(users.len(), 1, "member should see only own tenant users");
    assert_eq!(users[0].username, "user_t1");
}

// ── Tests: create ───────────────────────────────────────────────────

#[sqlx::test(migrations = "src/migrations")]
async fn test_create_success(pool: PgPool) {
    let admin = super_admin();
    let t1 = seed_tenant(&pool).await;

    let created = user::create(
        &pool,
        &admin,
        CreateUserRequest {
            username: "newuser".to_string(),
            password: "securepass123".to_string(),
            role: "member".to_string(),
            tenant_id: Some(t1),
            email: Some("new@example.com".to_string()),
        },
    )
    .await
    .unwrap();

    assert_eq!(created.username, "newuser");
    assert_eq!(created.role, "member");
    assert_eq!(created.tenant_id, Some(t1));
    assert_eq!(created.email.as_deref(), Some("new@example.com"));
    assert!(created.is_active);
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_create_forbidden_for_member(pool: PgPool) {
    let t1 = seed_tenant(&pool).await;
    let m = member(t1);

    let result = user::create(
        &pool,
        &m,
        CreateUserRequest {
            username: "hacker".to_string(),
            password: "longpassword".to_string(),
            role: "member".to_string(),
            tenant_id: Some(t1),
            email: None,
        },
    )
    .await;

    assert!(result.is_err());
    match result.unwrap_err() {
        AppError::Forbidden(_) => {}
        other => panic!("Expected Forbidden, got {:?}", other),
    }
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_create_short_password_rejected(pool: PgPool) {
    let admin = super_admin();
    let t1 = seed_tenant(&pool).await;

    let result = user::create(
        &pool,
        &admin,
        CreateUserRequest {
            username: "shortpw".to_string(),
            password: "abc".to_string(),
            role: "member".to_string(),
            tenant_id: Some(t1),
            email: None,
        },
    )
    .await;

    assert!(result.is_err());
    match result.unwrap_err() {
        AppError::BadRequest(msg) => {
            assert!(
                msg.contains("8 characters"),
                "Expected password length message, got: {}",
                msg
            );
        }
        other => panic!("Expected BadRequest, got {:?}", other),
    }
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_create_duplicate_username_conflict(pool: PgPool) {
    let admin = super_admin();
    let t1 = seed_tenant(&pool).await;

    user::create(
        &pool,
        &admin,
        CreateUserRequest {
            username: "dupuser".to_string(),
            password: "password1234".to_string(),
            role: "member".to_string(),
            tenant_id: Some(t1),
            email: None,
        },
    )
    .await
    .unwrap();

    let result = user::create(
        &pool,
        &admin,
        CreateUserRequest {
            username: "dupuser".to_string(),
            password: "password5678".to_string(),
            role: "member".to_string(),
            tenant_id: Some(t1),
            email: None,
        },
    )
    .await;

    assert!(result.is_err());
    match result.unwrap_err() {
        AppError::Conflict(msg) => {
            assert!(
                msg.contains("already exists"),
                "Expected 'already exists' message, got: {}",
                msg
            );
        }
        other => panic!("Expected Conflict, got {:?}", other),
    }
}

// ── Tests: invite ───────────────────────────────────────────────────

#[sqlx::test(migrations = "src/migrations")]
async fn test_invite_success(pool: PgPool) {
    let admin = super_admin();
    let t1 = seed_tenant(&pool).await;

    let invited = user::invite(
        &pool,
        &admin,
        InviteUserRequest {
            email: "invited@example.com".to_string(),
            role: Some("member".to_string()),
            tenant_id: Some(t1),
        },
    )
    .await
    .unwrap();

    assert_eq!(invited.email.as_deref(), Some("invited@example.com"));
    assert_eq!(invited.role, "member");
    assert_eq!(invited.tenant_id, Some(t1));
    assert_eq!(invited.auth_method, "invited");
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_invite_invalid_email_rejected(pool: PgPool) {
    let admin = super_admin();

    let result = user::invite(
        &pool,
        &admin,
        InviteUserRequest {
            email: "not-an-email".to_string(),
            role: None,
            tenant_id: None,
        },
    )
    .await;

    assert!(result.is_err());
    match result.unwrap_err() {
        AppError::BadRequest(msg) => {
            assert!(
                msg.contains("email"),
                "Expected email validation message, got: {}",
                msg
            );
        }
        other => panic!("Expected BadRequest, got {:?}", other),
    }
}

// ── Tests: delete ───────────────────────────────────────────────────

#[sqlx::test(migrations = "src/migrations")]
async fn test_delete_cannot_delete_self(pool: PgPool) {
    let t1 = seed_tenant(&pool).await;

    // Insert the user in DB so we have a real user_id
    let user_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO users (username, password_hash, role, tenant_id) \
         VALUES ('self_admin', '$2b$10$test', 'super_admin', $1) RETURNING id",
    )
    .bind(t1)
    .fetch_one(&pool)
    .await
    .unwrap();

    let admin = AuthUser {
        user_id,
        role: "super_admin".to_string(),
        tenant_id: None,
        username: "self_admin".to_string(),
    };

    let result = user::delete(&pool, &admin, user_id).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        AppError::BadRequest(msg) => {
            assert!(
                msg.contains("yourself"),
                "Expected self-deletion message, got: {}",
                msg
            );
        }
        other => panic!("Expected BadRequest, got {:?}", other),
    }
}
