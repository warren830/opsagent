mod helpers;

use sqlx::PgPool;
use ops::middleware::auth::AuthUser;
use ops::services::knowledge;
use ops::models::knowledge::{CreateKnowledgeRequest, UpdateKnowledgeRequest};
use ops::error::AppError;
use uuid::Uuid;

/// Seed a user in the DB and return an AuthUser with the real user_id.
async fn seed_super_admin(pool: &PgPool) -> AuthUser {
    let user_id: Uuid = sqlx::query_scalar(
        "INSERT INTO users (username, password_hash, role) VALUES ('admin', '$2b$12$dummy', 'super_admin') RETURNING id",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    AuthUser {
        user_id,
        role: "super_admin".to_string(),
        tenant_id: None,
        username: "admin".to_string(),
    }
}

async fn seed_tenant_admin(pool: &PgPool, tenant_id: Uuid) -> AuthUser {
    let user_id: Uuid = sqlx::query_scalar(
        "INSERT INTO users (username, password_hash, role, tenant_id) VALUES ('ta', '$2b$12$dummy', 'tenant_admin', $1) RETURNING id",
    )
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .unwrap();
    AuthUser {
        user_id,
        role: "tenant_admin".to_string(),
        tenant_id: Some(tenant_id),
        username: "ta".to_string(),
    }
}

async fn seed_member(pool: &PgPool, tenant_id: Uuid) -> AuthUser {
    let user_id: Uuid = sqlx::query_scalar(
        "INSERT INTO users (username, password_hash, role, tenant_id) VALUES ('m', '$2b$12$dummy', 'member', $1) RETURNING id",
    )
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .unwrap();
    AuthUser {
        user_id,
        role: "member".to_string(),
        tenant_id: Some(tenant_id),
        username: "m".to_string(),
    }
}

async fn seed_tenant(pool: &PgPool) -> Uuid {
    sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ('t', 't') RETURNING id")
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed_account(pool: &PgPool, tenant_id: Option<Uuid>) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO cloud_accounts (provider, name, tenant_id, config, regions, source) VALUES ('aws', 'test', $1, '{}', '{}', 'manual') RETURNING id",
    )
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_list_empty(pool: PgPool) {
    let admin = seed_super_admin(&pool).await;
    let result = knowledge::list(&pool, &admin).await.unwrap();
    assert!(result.is_empty());
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_create_and_list(pool: PgPool) {
    let admin = seed_super_admin(&pool).await;

    let created = knowledge::create(&pool, &admin, CreateKnowledgeRequest {
        filename: "runbook.md".to_string(),
        content: "# Runbook\nStep 1: check logs".to_string(),
        mime_type: Some("text/markdown".to_string()),
        account_id: None,
    }).await.unwrap();

    assert_eq!(created.filename, "runbook.md");
    assert_eq!(created.size_bytes, 28); // length of content
    assert_eq!(created.tenant_id, None); // super_admin has no tenant

    let all = knowledge::list(&pool, &admin).await.unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].id, created.id);
    assert_eq!(all[0].filename, "runbook.md");
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_create_empty_filename_rejected(pool: PgPool) {
    let admin = seed_super_admin(&pool).await;

    let result = knowledge::create(&pool, &admin, CreateKnowledgeRequest {
        filename: "".to_string(),
        content: "content".to_string(),
        mime_type: None,
        account_id: None,
    }).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        AppError::BadRequest(msg) => assert!(msg.contains("Filename"), "Expected 'Filename' in message, got: {}", msg),
        other => panic!("Expected BadRequest, got {:?}", other),
    }
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_delete_non_admin_no_account_forbidden(pool: PgPool) {
    let admin = seed_super_admin(&pool).await;

    // Create a global knowledge file (no account_id)
    let created = knowledge::create(&pool, &admin, CreateKnowledgeRequest {
        filename: "global.md".to_string(),
        content: "global content".to_string(),
        mime_type: None,
        account_id: None,
    }).await.unwrap();

    // A member tries to delete a global file -> should be forbidden
    let tid = seed_tenant(&pool).await;
    let m = seed_member(&pool, tid).await;

    let result = knowledge::delete(&pool, &m, created.id).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        AppError::Forbidden(_) => {}
        other => panic!("Expected Forbidden, got {:?}", other),
    }
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_create_with_account_id(pool: PgPool) {
    let tid = seed_tenant(&pool).await;
    let admin = seed_tenant_admin(&pool, tid).await;
    let account_id = seed_account(&pool, Some(tid)).await;

    let created = knowledge::create(&pool, &admin, CreateKnowledgeRequest {
        filename: "account-doc.md".to_string(),
        content: "account specific".to_string(),
        mime_type: None,
        account_id: Some(account_id),
    }).await.unwrap();

    assert_eq!(created.account_id, Some(account_id));
    // tenant_id should be derived from the account
    assert_eq!(created.tenant_id, Some(tid));
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_update_success(pool: PgPool) {
    let admin = seed_super_admin(&pool).await;

    let created = knowledge::create(&pool, &admin, CreateKnowledgeRequest {
        filename: "old-name.md".to_string(),
        content: "some content".to_string(),
        mime_type: None,
        account_id: None,
    }).await.unwrap();

    assert_eq!(created.filename, "old-name.md");

    let updated = knowledge::update(&pool, &admin, created.id, UpdateKnowledgeRequest {
        filename: Some("new-name.md".to_string()),
        content: None,
        mime_type: None,
        account_id: None,
    }).await.unwrap();

    assert_eq!(updated.filename, "new-name.md");
    assert_eq!(updated.id, created.id);

    // Verify persistence via list
    let all = knowledge::list(&pool, &admin).await.unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].filename, "new-name.md");
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_update_not_found(pool: PgPool) {
    let admin = seed_super_admin(&pool).await;

    let result = knowledge::update(&pool, &admin, Uuid::new_v4(), UpdateKnowledgeRequest {
        filename: Some("ghost.md".to_string()),
        content: None,
        mime_type: None,
        account_id: None,
    }).await;

    assert!(matches!(result, Err(AppError::NotFound(_))));
}
