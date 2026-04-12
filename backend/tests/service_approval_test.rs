mod helpers;

use sqlx::PgPool;
use uuid::Uuid;

use openops::error::AppError;
use openops::middleware::auth::AuthUser;
use openops::services::approval;

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

async fn seed_approval(pool: &PgPool, tenant_id: Option<Uuid>, requested_by: Uuid) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO approvals (command, requested_by, tenant_id, status) \
         VALUES ('kubectl delete pod x', $1, $2, 'pending') RETURNING id",
    )
    .bind(requested_by)
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn seed_approval_with_status(
    pool: &PgPool,
    tenant_id: Option<Uuid>,
    requested_by: Uuid,
    status: &str,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO approvals (command, requested_by, tenant_id, status) \
         VALUES ('kubectl rollout restart deploy/app', $1, $2, $3) RETURNING id",
    )
    .bind(requested_by)
    .bind(tenant_id)
    .bind(status)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Seed a super_admin user in the DB and return an AuthUser with matching user_id.
/// Needed because approve/reject write `reviewed_by` which has FK to users.
async fn seed_super_admin_auth(pool: &PgPool, username: &str) -> AuthUser {
    let user_id = sqlx::query_scalar(
        "INSERT INTO users (username, password_hash, role) \
         VALUES ($1, '$2b$10$test', 'super_admin') RETURNING id",
    )
    .bind(username)
    .fetch_one(pool)
    .await
    .unwrap();
    AuthUser {
        user_id,
        role: "super_admin".to_string(),
        tenant_id: None,
        username: username.to_string(),
    }
}

// ── Tests: list ─────────────────────────────────────────────────────

#[sqlx::test(migrations = "src/migrations")]
async fn test_list_super_admin_sees_all(pool: PgPool) {
    let t1 = seed_tenant_named(&pool, "Tenant A", "ta").await;
    let t2 = seed_tenant_named(&pool, "Tenant B", "tb").await;

    let u1 = seed_user_record(&pool, "user1", Some(t1)).await;
    let u2 = seed_user_record(&pool, "user2", Some(t2)).await;

    let _a1 = seed_approval(&pool, Some(t1), u1).await;
    let _a2 = seed_approval(&pool, Some(t2), u2).await;

    let admin = super_admin();
    let all = approval::list(&pool, &admin, None).await.unwrap();
    assert_eq!(all.len(), 2, "super_admin should see approvals from all tenants");
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_list_member_sees_own_tenant(pool: PgPool) {
    let t1 = seed_tenant_named(&pool, "Tenant A", "ta").await;
    let t2 = seed_tenant_named(&pool, "Tenant B", "tb").await;

    let u1 = seed_user_record(&pool, "user1", Some(t1)).await;
    let u2 = seed_user_record(&pool, "user2", Some(t2)).await;

    let _a1 = seed_approval(&pool, Some(t1), u1).await;
    let _a2 = seed_approval(&pool, Some(t2), u2).await;

    let m = member(t1);
    let visible = approval::list(&pool, &m, None).await.unwrap();
    assert_eq!(visible.len(), 1, "member should see only own tenant approvals");
    assert_eq!(visible[0].tenant_id, Some(t1));
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_list_filter_by_status(pool: PgPool) {
    let t1 = seed_tenant(&pool).await;
    let u1 = seed_user_record(&pool, "user1", Some(t1)).await;

    let _pending = seed_approval(&pool, Some(t1), u1).await;
    let _approved = seed_approval_with_status(&pool, Some(t1), u1, "approved").await;
    let _rejected = seed_approval_with_status(&pool, Some(t1), u1, "rejected").await;

    let admin = super_admin();

    let pending_only = approval::list(&pool, &admin, Some("pending")).await.unwrap();
    assert_eq!(pending_only.len(), 1);
    assert_eq!(pending_only[0].status, "pending");

    let approved_only = approval::list(&pool, &admin, Some("approved")).await.unwrap();
    assert_eq!(approved_only.len(), 1);
    assert_eq!(approved_only[0].status, "approved");
}

// ── Tests: approve ──────────────────────────────────────────────────

#[sqlx::test(migrations = "src/migrations")]
async fn test_approve_success(pool: PgPool) {
    let t1 = seed_tenant(&pool).await;
    let u1 = seed_user_record(&pool, "requester", Some(t1)).await;
    let approval_id = seed_approval(&pool, Some(t1), u1).await;

    let admin = seed_super_admin_auth(&pool, "approver_admin").await;
    let result = approval::approve(&pool, &admin, approval_id).await.unwrap();

    assert_eq!(result.status, "approved");
    assert_eq!(result.reviewed_by, Some(admin.user_id));
    assert!(result.reviewed_at.is_some());
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_approve_already_processed(pool: PgPool) {
    let t1 = seed_tenant(&pool).await;
    let u1 = seed_user_record(&pool, "requester", Some(t1)).await;
    let approval_id = seed_approval(&pool, Some(t1), u1).await;

    let admin = seed_super_admin_auth(&pool, "approver_admin").await;
    // Approve once — should succeed
    approval::approve(&pool, &admin, approval_id).await.unwrap();

    // Approve again — should fail because status is no longer 'pending'
    let result = approval::approve(&pool, &admin, approval_id).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        AppError::NotFound(msg) => {
            assert!(
                msg.contains("already processed") || msg.contains("not found"),
                "Expected already-processed message, got: {}",
                msg
            );
        }
        other => panic!("Expected NotFound, got {:?}", other),
    }
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_approve_other_tenant_forbidden(pool: PgPool) {
    let t1 = seed_tenant_named(&pool, "Tenant A", "ta").await;
    let t2 = seed_tenant_named(&pool, "Tenant B", "tb").await;

    let u1 = seed_user_record(&pool, "requester_t1", Some(t1)).await;
    let approval_id = seed_approval(&pool, Some(t1), u1).await;

    // Member from t2 tries to approve t1's approval
    let m_t2 = member(t2);
    let result = approval::approve(&pool, &m_t2, approval_id).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        AppError::Forbidden(_) => {}
        other => panic!("Expected Forbidden, got {:?}", other),
    }
}

// ── Tests: reject ───────────────────────────────────────────────────

#[sqlx::test(migrations = "src/migrations")]
async fn test_reject_success(pool: PgPool) {
    let t1 = seed_tenant(&pool).await;
    let u1 = seed_user_record(&pool, "requester", Some(t1)).await;
    let approval_id = seed_approval(&pool, Some(t1), u1).await;

    let admin = seed_super_admin_auth(&pool, "rejector_admin").await;
    let result = approval::reject(&pool, &admin, approval_id).await.unwrap();

    assert_eq!(result.status, "rejected");
    assert_eq!(result.reviewed_by, Some(admin.user_id));
    assert!(result.reviewed_at.is_some());
}
