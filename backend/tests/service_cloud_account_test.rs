use sqlx::PgPool;
use uuid::Uuid;

use ops::error::AppError;
use ops::middleware::auth::AuthUser;
use ops::models::cloud_account::{CreateCloudAccountRequest, UpdateCloudAccountRequest};
use ops::services::cloud_account;

fn super_admin() -> AuthUser {
    AuthUser {
        user_id: Uuid::new_v4(),
        role: "super_admin".to_string(),
        tenant_id: None,
        username: "admin".to_string(),
    }
}

fn super_admin_with_tenant(tenant_id: Uuid) -> AuthUser {
    AuthUser {
        user_id: Uuid::new_v4(),
        role: "super_admin".to_string(),
        tenant_id: Some(tenant_id),
        username: "admin".to_string(),
    }
}

async fn seed_tenant(pool: &PgPool) -> Uuid {
    sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ('t', 't') RETURNING id")
        .fetch_one(pool)
        .await
        .unwrap()
}

fn make_req(provider: &str, name: &str) -> CreateCloudAccountRequest {
    CreateCloudAccountRequest {
        provider: provider.to_string(),
        name: name.to_string(),
        account_id: Some("111122223333".to_string()),
        config: serde_json::json!({}),
        secret_arn: None,
        role_arn: None,
        profile: None,
        regions: Some(vec!["us-east-1".to_string()]),
        source: Some("manual".to_string()),
        tenant_id: None,
        is_mock: false,
        discover_org: false,
    }
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_list_super_admin_sees_all(pool: PgPool) {
    let tid = seed_tenant(&pool).await;
    let admin = super_admin_with_tenant(tid);

    cloud_account::create(&pool, &admin, make_req("aws", "Account A")).await.unwrap();

    let mut req2 = make_req("aws", "Account B");
    req2.account_id = Some("222233334444".to_string());
    cloud_account::create(&pool, &admin, req2).await.unwrap();

    let plain_admin = super_admin();
    let accounts = cloud_account::list(&pool, &plain_admin).await.unwrap();
    assert_eq!(accounts.len(), 2);
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_create_success(pool: PgPool) {
    let tid = seed_tenant(&pool).await;
    let admin = super_admin_with_tenant(tid);

    let account = cloud_account::create(&pool, &admin, make_req("aws", "My AWS"))
        .await
        .unwrap();

    assert_eq!(account.name, "My AWS");
    assert_eq!(account.provider, "aws");
    assert_eq!(account.tenant_id, Some(tid));
    assert!(!account.is_mock);
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_create_empty_name_rejected(pool: PgPool) {
    let admin = super_admin();

    let result = cloud_account::create(&pool, &admin, make_req("aws", "")).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        AppError::BadRequest(msg) => assert!(msg.contains("Name"), "got: {}", msg),
        other => panic!("Expected BadRequest, got {:?}", other),
    }
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_create_empty_provider_rejected(pool: PgPool) {
    let admin = super_admin();

    let result = cloud_account::create(&pool, &admin, make_req("", "Valid Name")).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        AppError::BadRequest(msg) => assert!(msg.contains("Provider"), "got: {}", msg),
        other => panic!("Expected BadRequest, got {:?}", other),
    }
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_update_success(pool: PgPool) {
    let tid = seed_tenant(&pool).await;
    let admin = super_admin_with_tenant(tid);

    let account = cloud_account::create(&pool, &admin, make_req("aws", "Original"))
        .await
        .unwrap();

    let updated = cloud_account::update(
        &pool,
        &admin,
        account.id,
        UpdateCloudAccountRequest {
            provider: None,
            name: Some("Renamed".to_string()),
            account_id: None,
            config: None,
            secret_arn: None,
            role_arn: None,
            profile: None,
            regions: None,
            is_mock: None,
            tenant_id: Some(tid),
        },
    )
    .await
    .unwrap();

    assert_eq!(updated.name, "Renamed");
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_delete_success(pool: PgPool) {
    let tid = seed_tenant(&pool).await;
    let admin = super_admin_with_tenant(tid);

    let account = cloud_account::create(&pool, &admin, make_req("aws", "Delete Me"))
        .await
        .unwrap();

    cloud_account::delete(&pool, &admin, account.id).await.unwrap();

    let accounts = cloud_account::list(&pool, &admin).await.unwrap();
    assert!(accounts.is_empty());
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_seed_mock(pool: PgPool) {
    let tid = seed_tenant(&pool).await;
    let admin = super_admin_with_tenant(tid);

    let mocks = cloud_account::seed_mock(&pool, &admin).await.unwrap();
    assert_eq!(mocks.len(), 2);

    let providers: Vec<&str> = mocks.iter().map(|a| a.provider.as_str()).collect();
    assert!(providers.contains(&"alicloud"));
    assert!(providers.contains(&"azure"));

    for m in &mocks {
        assert!(m.is_mock);
        assert_eq!(m.tenant_id, Some(tid));
    }
}
