mod helpers;

use sqlx::PgPool;
use uuid::Uuid;

use ops::middleware::auth::AuthUser;
use ops::models::account_access::GrantAccessRequest;
use ops::services::account_access;

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

fn tenant_admin(tenant_id: Uuid) -> AuthUser {
    AuthUser {
        user_id: Uuid::new_v4(),
        role: "tenant_admin".to_string(),
        tenant_id: Some(tenant_id),
        username: "test_ta".to_string(),
    }
}

fn member_with_id(user_id: Uuid, tenant_id: Uuid) -> AuthUser {
    AuthUser {
        user_id,
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

async fn seed_account(pool: &PgPool, name: &str, tenant_id: Option<Uuid>) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO cloud_accounts (provider, name, tenant_id, config, regions, source) \
         VALUES ('aws', $1, $2, '{}', '{}', 'manual') RETURNING id",
    )
    .bind(name)
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn seed_user_record(pool: &PgPool, username: &str, tenant_id: Option<Uuid>) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO users (username, password_hash, role, tenant_id) \
         VALUES ($1, '$2b$10$test', 'member', $2) RETURNING id",
    )
    .bind(username)
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn grant_access(pool: &PgPool, user_id: Uuid, account_id: Uuid, role: &str) {
    sqlx::query("INSERT INTO user_account_access (user_id, account_id, role) VALUES ($1, $2, $3)")
        .bind(user_id)
        .bind(account_id)
        .bind(role)
        .execute(pool)
        .await
        .unwrap();
}

/// Seed a user record in DB and return an AuthUser with matching user_id.
async fn seed_member_auth(pool: &PgPool, username: &str, tenant_id: Uuid) -> AuthUser {
    let user_id = seed_user_record(pool, username, Some(tenant_id)).await;
    member_with_id(user_id, tenant_id)
}

async fn seed_tenant_admin_auth(pool: &PgPool, username: &str, tenant_id: Uuid) -> AuthUser {
    let user_id = sqlx::query_scalar(
        "INSERT INTO users (username, password_hash, role, tenant_id) \
         VALUES ($1, '$2b$10$test', 'tenant_admin', $2) RETURNING id",
    )
    .bind(username)
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .unwrap();
    AuthUser {
        user_id,
        role: "tenant_admin".to_string(),
        tenant_id: Some(tenant_id),
        username: username.to_string(),
    }
}

// ── Tests: get_accessible_account_ids ───────────────────────────────

#[sqlx::test(migrations = "src/migrations")]
async fn test_get_accessible_account_ids_super_admin(pool: PgPool) {
    let t1 = seed_tenant(&pool).await;
    let _a1 = seed_account(&pool, "acct-1", Some(t1)).await;
    let _a2 = seed_account(&pool, "acct-2", None).await;

    let admin = super_admin();
    let ids = account_access::get_accessible_account_ids(&pool, &admin).await;
    assert_eq!(ids.len(), 2, "super_admin should see all accounts");
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_get_accessible_account_ids_tenant_admin(pool: PgPool) {
    let t1 = seed_tenant_named(&pool, "Tenant A", "ta").await;
    let t2 = seed_tenant_named(&pool, "Tenant B", "tb").await;

    let a1 = seed_account(&pool, "acct-t1", Some(t1)).await;
    let a2 = seed_account(&pool, "acct-t2", Some(t2)).await;
    let a3 = seed_account(&pool, "acct-none", None).await;

    // Create a real user record for the tenant_admin so FK works
    let ta = seed_tenant_admin_auth(&pool, "ta_user", t1).await;
    // Grant explicit access to account in t2
    grant_access(&pool, ta.user_id, a2, "readonly").await;

    let ids = account_access::get_accessible_account_ids(&pool, &ta).await;
    assert!(ids.contains(&a1), "should see own tenant account");
    assert!(ids.contains(&a2), "should see explicitly granted account");
    assert!(!ids.contains(&a3), "should NOT see unrelated account");
    assert_eq!(ids.len(), 2);
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_get_accessible_account_ids_member(pool: PgPool) {
    let t1 = seed_tenant(&pool).await;

    let a1 = seed_account(&pool, "acct-1", Some(t1)).await;
    let a2 = seed_account(&pool, "acct-2", Some(t1)).await;
    let _a3 = seed_account(&pool, "acct-3", Some(t1)).await;

    // Create real user record so FK on user_account_access works
    let m = seed_member_auth(&pool, "member_user", t1).await;
    // Only grant access to a1 and a2
    grant_access(&pool, m.user_id, a1, "readonly").await;
    grant_access(&pool, m.user_id, a2, "admin").await;

    let ids = account_access::get_accessible_account_ids(&pool, &m).await;
    assert_eq!(ids.len(), 2, "member should see only explicitly granted accounts");
    assert!(ids.contains(&a1));
    assert!(ids.contains(&a2));
}

// ── Tests: can_write_account ────────────────────────────────────────

#[sqlx::test(migrations = "src/migrations")]
async fn test_can_write_account_super_admin(pool: PgPool) {
    let t1 = seed_tenant(&pool).await;
    let a1 = seed_account(&pool, "acct-1", Some(t1)).await;

    let admin = super_admin();
    let result = account_access::can_write_account(&pool, &admin, a1).await;
    assert!(result, "super_admin should always have write access");
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_can_write_account_explicit_admin_grant(pool: PgPool) {
    let t1 = seed_tenant(&pool).await;
    let a1 = seed_account(&pool, "acct-1", Some(t1)).await;

    let m = seed_member_auth(&pool, "admin_grantee", t1).await;
    grant_access(&pool, m.user_id, a1, "admin").await;

    let result = account_access::can_write_account(&pool, &m, a1).await;
    assert!(result, "explicit admin grant should allow write");
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_can_write_account_explicit_readonly_grant(pool: PgPool) {
    let t1 = seed_tenant(&pool).await;
    let a1 = seed_account(&pool, "acct-1", Some(t1)).await;

    let m = seed_member_auth(&pool, "readonly_grantee", t1).await;
    grant_access(&pool, m.user_id, a1, "readonly").await;

    let result = account_access::can_write_account(&pool, &m, a1).await;
    assert!(!result, "explicit readonly grant should deny write");
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_can_write_account_tenant_admin_own_tenant(pool: PgPool) {
    let t1 = seed_tenant(&pool).await;
    let a1 = seed_account(&pool, "acct-1", Some(t1)).await;

    let ta = tenant_admin(t1);
    // No explicit grant — tenant_admin should still write to own tenant's accounts
    let result = account_access::can_write_account(&pool, &ta, a1).await;
    assert!(result, "tenant_admin should write own tenant accounts without explicit grant");
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_can_write_account_member_no_grant(pool: PgPool) {
    let t1 = seed_tenant(&pool).await;
    let a1 = seed_account(&pool, "acct-1", Some(t1)).await;

    let m = member(t1);
    // No grant at all
    let result = account_access::can_write_account(&pool, &m, a1).await;
    assert!(!result, "member with no grant should not have write access");
}

// ── Test: grant and revoke lifecycle ────────────────────────────────

#[sqlx::test(migrations = "src/migrations")]
async fn test_grant_and_revoke(pool: PgPool) {
    let t1 = seed_tenant(&pool).await;
    let a1 = seed_account(&pool, "acct-1", Some(t1)).await;
    let user_id = seed_user_record(&pool, "grantee", Some(t1)).await;

    let admin = super_admin();
    let m = member_with_id(user_id, t1);

    // Before grant — no write
    assert!(!account_access::can_write_account(&pool, &m, a1).await);

    // Grant admin access via the service function
    let access = account_access::grant(
        &pool,
        &admin,
        GrantAccessRequest {
            user_id,
            account_id: a1,
            role: "admin".to_string(),
        },
    )
    .await
    .unwrap();
    assert_eq!(access.role, "admin");

    // After grant — can write
    assert!(account_access::can_write_account(&pool, &m, a1).await);

    // Revoke
    account_access::revoke(&pool, &admin, user_id, a1).await.unwrap();

    // After revoke — no write
    assert!(!account_access::can_write_account(&pool, &m, a1).await);
}
