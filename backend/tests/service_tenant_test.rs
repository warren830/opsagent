mod helpers;

use sqlx::PgPool;
use ops::middleware::auth::AuthUser;
use ops::services::tenant;
use ops::error::AppError;
use ops::models::tenant::{CreateTenantRequest, UpdateTenantRequest};
use uuid::Uuid;

// helper to create a test super_admin AuthUser
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

#[sqlx::test(migrations = "src/migrations")]
async fn test_list_as_super_admin(pool: PgPool) {
    let admin = super_admin();

    // Seed 2 tenants
    let t1 = tenant::create(&pool, &admin, CreateTenantRequest {
        name: "Tenant Alpha".to_string(),
        slug: "alpha".to_string(),
        aws_account_ids: vec![],
        settings: serde_json::Value::Null,
    }).await.unwrap();

    let t2 = tenant::create(&pool, &admin, CreateTenantRequest {
        name: "Tenant Beta".to_string(),
        slug: "beta".to_string(),
        aws_account_ids: vec![],
        settings: serde_json::Value::Null,
    }).await.unwrap();

    let tenants = tenant::list(&pool, &admin).await.unwrap();
    assert_eq!(tenants.len(), 2);

    let ids: Vec<Uuid> = tenants.iter().map(|t| t.id).collect();
    assert!(ids.contains(&t1.id));
    assert!(ids.contains(&t2.id));
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_list_as_member(pool: PgPool) {
    let admin = super_admin();

    let t1 = tenant::create(&pool, &admin, CreateTenantRequest {
        name: "Tenant One".to_string(),
        slug: "one".to_string(),
        aws_account_ids: vec![],
        settings: serde_json::Value::Null,
    }).await.unwrap();

    let _t2 = tenant::create(&pool, &admin, CreateTenantRequest {
        name: "Tenant Two".to_string(),
        slug: "two".to_string(),
        aws_account_ids: vec![],
        settings: serde_json::Value::Null,
    }).await.unwrap();

    let member_user = member(t1.id);
    let tenants = tenant::list(&pool, &member_user).await.unwrap();
    assert_eq!(tenants.len(), 1);
    assert_eq!(tenants[0].id, t1.id);
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_create_success(pool: PgPool) {
    let admin = super_admin();

    let created = tenant::create(&pool, &admin, CreateTenantRequest {
        name: "Acme Corp".to_string(),
        slug: "acme".to_string(),
        aws_account_ids: vec!["123456789012".to_string()],
        settings: serde_json::json!({"region": "us-east-1"}),
    }).await.unwrap();

    assert_eq!(created.name, "Acme Corp");
    assert_eq!(created.slug, "acme");
    assert_eq!(created.aws_account_ids, vec!["123456789012".to_string()]);
    assert_eq!(created.settings, serde_json::json!({"region": "us-east-1"}));
    assert!(created.id != Uuid::nil());
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_create_forbidden_for_member(pool: PgPool) {
    let member_user = member(Uuid::new_v4());

    let result = tenant::create(&pool, &member_user, CreateTenantRequest {
        name: "Forbidden Tenant".to_string(),
        slug: "forbidden".to_string(),
        aws_account_ids: vec![],
        settings: serde_json::Value::Null,
    }).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        AppError::Forbidden(_) => {}
        other => panic!("Expected Forbidden, got {:?}", other),
    }
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_create_empty_name_rejected(pool: PgPool) {
    let admin = super_admin();

    let result = tenant::create(&pool, &admin, CreateTenantRequest {
        name: "".to_string(),
        slug: "valid-slug".to_string(),
        aws_account_ids: vec![],
        settings: serde_json::Value::Null,
    }).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        AppError::BadRequest(msg) => {
            assert!(msg.contains("name"), "Expected 'name' in message, got: {}", msg);
        }
        other => panic!("Expected BadRequest, got {:?}", other),
    }
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_create_duplicate_slug_conflict(pool: PgPool) {
    let admin = super_admin();

    tenant::create(&pool, &admin, CreateTenantRequest {
        name: "First Tenant".to_string(),
        slug: "duplicate".to_string(),
        aws_account_ids: vec![],
        settings: serde_json::Value::Null,
    }).await.unwrap();

    let result = tenant::create(&pool, &admin, CreateTenantRequest {
        name: "Second Tenant".to_string(),
        slug: "duplicate".to_string(),
        aws_account_ids: vec![],
        settings: serde_json::Value::Null,
    }).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        AppError::Conflict(msg) => {
            assert!(msg.contains("already exists"), "Expected 'already exists' in message, got: {}", msg);
        }
        other => panic!("Expected Conflict, got {:?}", other),
    }
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_get_access_denied(pool: PgPool) {
    let admin = super_admin();

    let t1 = tenant::create(&pool, &admin, CreateTenantRequest {
        name: "Tenant A".to_string(),
        slug: "tenant-a".to_string(),
        aws_account_ids: vec![],
        settings: serde_json::Value::Null,
    }).await.unwrap();

    let t2 = tenant::create(&pool, &admin, CreateTenantRequest {
        name: "Tenant B".to_string(),
        slug: "tenant-b".to_string(),
        aws_account_ids: vec![],
        settings: serde_json::Value::Null,
    }).await.unwrap();

    // Member of tenant1 CAN access their own tenant
    let member_of_t1 = member(t1.id);
    let own = tenant::get(&pool, &member_of_t1, t1.id).await.unwrap();
    assert_eq!(own.id, t1.id);
    assert_eq!(own.name, "Tenant A");

    // Member of tenant1 tries to access tenant2 — should be denied
    let result = tenant::get(&pool, &member_of_t1, t2.id).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        AppError::Forbidden(_) => {}
        other => panic!("Expected Forbidden, got {:?}", other),
    }
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_delete_success(pool: PgPool) {
    let admin = super_admin();

    let created = tenant::create(&pool, &admin, CreateTenantRequest {
        name: "Delete Me".to_string(),
        slug: "delete-me".to_string(),
        aws_account_ids: vec![],
        settings: serde_json::Value::Null,
    }).await.unwrap();

    tenant::delete(&pool, &admin, created.id).await.unwrap();

    // Verify it's gone
    let result = tenant::get(&pool, &admin, created.id).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        AppError::NotFound(_) => {}
        other => panic!("Expected NotFound, got {:?}", other),
    }
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_delete_forbidden_for_member(pool: PgPool) {
    let admin = super_admin();

    let created = tenant::create(&pool, &admin, CreateTenantRequest {
        name: "No Delete".to_string(),
        slug: "no-delete".to_string(),
        aws_account_ids: vec![],
        settings: serde_json::Value::Null,
    }).await.unwrap();

    let member_user = member(created.id);
    let result = tenant::delete(&pool, &member_user, created.id).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        AppError::Forbidden(_) => {}
        other => panic!("Expected Forbidden, got {:?}", other),
    }
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_update_success(pool: PgPool) {
    let admin = super_admin();

    let created = tenant::create(&pool, &admin, CreateTenantRequest {
        name: "Original Name".to_string(),
        slug: "original".to_string(),
        aws_account_ids: vec![],
        settings: serde_json::Value::Null,
    }).await.unwrap();

    let updated = tenant::update(&pool, &admin, created.id, UpdateTenantRequest {
        name: Some("Updated Name".to_string()),
        slug: None,
        aws_account_ids: None,
        settings: None,
    }).await.unwrap();

    assert_eq!(updated.id, created.id);
    assert_eq!(updated.name, "Updated Name");
    assert_eq!(updated.slug, "original"); // unchanged

    // Verify persisted by re-fetching
    let fetched = tenant::get(&pool, &admin, created.id).await.unwrap();
    assert_eq!(fetched.name, "Updated Name");
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_update_not_found(pool: PgPool) {
    let admin = super_admin();

    let result = tenant::update(&pool, &admin, Uuid::new_v4(), UpdateTenantRequest {
        name: Some("Ghost".to_string()),
        slug: None,
        aws_account_ids: None,
        settings: None,
    }).await;

    assert!(matches!(result, Err(AppError::NotFound(_))));
}
