mod helpers;

use sqlx::PgPool;
use ops::middleware::auth::AuthUser;
use ops::services::provider;
use ops::models::provider::{CreateProviderRequest, UpdateProviderRequest};
use ops::error::AppError;
use uuid::Uuid;

fn super_admin() -> AuthUser {
    AuthUser {
        user_id: Uuid::new_v4(),
        role: "super_admin".to_string(),
        tenant_id: None,
        username: "admin".to_string(),
    }
}

fn admin_with_tenant(tenant_id: Uuid) -> AuthUser {
    AuthUser {
        user_id: Uuid::new_v4(),
        role: "tenant_admin".to_string(),
        tenant_id: Some(tenant_id),
        username: "ta".to_string(),
    }
}

fn member(tenant_id: Uuid) -> AuthUser {
    AuthUser {
        user_id: Uuid::new_v4(),
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

fn make_req(name: &str, is_default: bool) -> CreateProviderRequest {
    CreateProviderRequest {
        name: name.to_string(),
        provider_type: "gateway".to_string(),
        config: serde_json::json!({}),
        is_default,
    }
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_list_empty(pool: PgPool) {
    let admin = super_admin();
    let result = provider::list(&pool, &admin).await.unwrap();
    assert!(result.is_empty());
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_create_first_provider_forces_default(pool: PgPool) {
    let tid = seed_tenant(&pool).await;
    let admin = admin_with_tenant(tid);

    let p = provider::create(&pool, &admin, make_req("First", false)).await.unwrap();
    assert!(p.is_default, "First provider must become default even when is_default=false");
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_create_second_provider_not_default(pool: PgPool) {
    let tid = seed_tenant(&pool).await;
    let admin = admin_with_tenant(tid);

    let _first = provider::create(&pool, &admin, make_req("First", false)).await.unwrap();
    let second = provider::create(&pool, &admin, make_req("Second", false)).await.unwrap();
    assert!(!second.is_default, "Second provider with is_default=false should stay non-default");
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_create_set_default_unsets_previous(pool: PgPool) {
    let tid = seed_tenant(&pool).await;
    let admin = admin_with_tenant(tid);

    let first = provider::create(&pool, &admin, make_req("First", false)).await.unwrap();
    assert!(first.is_default);

    let second = provider::create(&pool, &admin, make_req("Second", true)).await.unwrap();
    assert!(second.is_default);

    // Re-fetch all and confirm only the second is default
    let all = provider::list(&pool, &admin).await.unwrap();
    for p in &all {
        if p.id == first.id {
            assert!(!p.is_default, "Previous default should have been unset");
        }
        if p.id == second.id {
            assert!(p.is_default);
        }
    }
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_create_empty_name_rejected(pool: PgPool) {
    let tid = seed_tenant(&pool).await;
    let admin = admin_with_tenant(tid);

    let result = provider::create(&pool, &admin, make_req("", false)).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        AppError::BadRequest(msg) => assert!(msg.contains("Name"), "Expected 'Name' in message, got: {}", msg),
        other => panic!("Expected BadRequest, got {:?}", other),
    }
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_create_non_admin_forbidden(pool: PgPool) {
    let tid = seed_tenant(&pool).await;
    let m = member(tid);

    let result = provider::create(&pool, &m, make_req("Nope", false)).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_delete_default_promotes_next(pool: PgPool) {
    let tid = seed_tenant(&pool).await;
    let admin = admin_with_tenant(tid);

    let first = provider::create(&pool, &admin, make_req("First", false)).await.unwrap();
    assert!(first.is_default);
    let second = provider::create(&pool, &admin, make_req("Second", false)).await.unwrap();
    assert!(!second.is_default);

    // Delete the default
    provider::delete(&pool, &admin, first.id).await.unwrap();

    // The remaining provider should now be default
    let all = provider::list(&pool, &admin).await.unwrap();
    assert_eq!(all.len(), 1);
    assert!(all[0].is_default, "After deleting default, next provider should be promoted");
    assert_eq!(all[0].id, second.id);
}

#[test]
fn test_available_types_local() {
    let types = provider::available_types(true);
    assert_eq!(types.len(), 2);
    let values: Vec<&str> = types.iter().map(|t| t.value.as_str()).collect();
    assert!(values.contains(&"bedrock"));
    assert!(values.contains(&"gateway"));
}

#[test]
fn test_available_types_non_local() {
    let types = provider::available_types(false);
    assert_eq!(types.len(), 1);
    assert_eq!(types[0].value, "gateway");
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_update_success(pool: PgPool) {
    let tid = seed_tenant(&pool).await;
    let admin = admin_with_tenant(tid);

    let p = provider::create(&pool, &admin, make_req("Original", false)).await.unwrap();
    assert_eq!(p.name, "Original");

    let updated = provider::update(
        &pool,
        &admin,
        p.id,
        UpdateProviderRequest {
            name: Some("Renamed".to_string()),
            provider_type: None,
            config: None,
            is_default: None,
        },
    )
    .await
    .unwrap();

    assert_eq!(updated.name, "Renamed");
    assert_eq!(updated.id, p.id);
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_update_not_found(pool: PgPool) {
    let tid = seed_tenant(&pool).await;
    let admin = admin_with_tenant(tid);

    let result = provider::update(
        &pool,
        &admin,
        Uuid::new_v4(),
        UpdateProviderRequest {
            name: Some("Ghost".to_string()),
            provider_type: None,
            config: None,
            is_default: None,
        },
    )
    .await;

    assert!(matches!(result, Err(AppError::NotFound(_))));
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_update_other_tenant_forbidden(pool: PgPool) {
    let tid1 = seed_tenant(&pool).await;
    let admin1 = admin_with_tenant(tid1);

    let p = provider::create(&pool, &admin1, make_req("Owned", false)).await.unwrap();

    // Different tenant admin
    let tid2: Uuid = sqlx::query_scalar(
        "INSERT INTO tenants (name, slug) VALUES ('t2', 't2') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let admin2 = admin_with_tenant(tid2);

    let result = provider::update(
        &pool,
        &admin2,
        p.id,
        UpdateProviderRequest {
            name: Some("Hacked".to_string()),
            provider_type: None,
            config: None,
            is_default: None,
        },
    )
    .await;

    assert!(matches!(result, Err(AppError::Forbidden(_))));
}
