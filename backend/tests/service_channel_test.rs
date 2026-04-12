mod helpers;

use sqlx::PgPool;
use ops::middleware::auth::AuthUser;
use ops::services::channel;
use ops::models::channel::{CreateChannelRequest, UpdateChannelRequest};
use ops::error::AppError;
use uuid::Uuid;

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
    let t1 = helpers::seed::seed_tenant(&pool, "Tenant A", "tenant-a").await;
    let t2 = helpers::seed::seed_tenant(&pool, "Tenant B", "tenant-b").await;

    helpers::seed::seed_channel(&pool, "Slack A", "slack", Some(t1)).await;
    helpers::seed::seed_channel(&pool, "Teams B", "teams", Some(t2)).await;

    let admin = super_admin();
    let channels = channel::list(&pool, &admin).await.unwrap();
    assert_eq!(channels.len(), 2);
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_list_as_member(pool: PgPool) {
    let t1 = helpers::seed::seed_tenant(&pool, "Tenant One", "tenant-one").await;
    let t2 = helpers::seed::seed_tenant(&pool, "Tenant Two", "tenant-two").await;

    helpers::seed::seed_channel(&pool, "My Slack", "slack", Some(t1)).await;
    helpers::seed::seed_channel(&pool, "Other Teams", "teams", Some(t2)).await;

    let member_user = member(t1);
    let channels = channel::list(&pool, &member_user).await.unwrap();
    assert_eq!(channels.len(), 1);
    assert_eq!(channels[0].name, "My Slack");
    assert_eq!(channels[0].tenant_id, Some(t1));
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_create_success(pool: PgPool) {
    let t1 = helpers::seed::seed_tenant(&pool, "Acme", "acme").await;
    let member_user = member(t1);

    let created = channel::create(&pool, &member_user, CreateChannelRequest {
        platform: "slack".to_string(),
        name: "Engineering".to_string(),
        credentials: serde_json::json!({"token": "xoxb-123"}),
        settings: serde_json::json!({"notify": true}),
        enabled: true,
    }).await.unwrap();

    assert_eq!(created.platform, "slack");
    assert_eq!(created.name, "Engineering");
    assert_eq!(created.tenant_id, Some(t1));
    assert!(created.enabled);
    assert_eq!(created.credentials, serde_json::json!({"token": "xoxb-123"}));
    assert_eq!(created.settings, serde_json::json!({"notify": true}));
    assert!(created.id != Uuid::nil());
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_create_empty_name_rejected(pool: PgPool) {
    let admin = super_admin();

    let result = channel::create(&pool, &admin, CreateChannelRequest {
        platform: "slack".to_string(),
        name: "".to_string(),
        credentials: serde_json::Value::Null,
        settings: serde_json::Value::Null,
        enabled: true,
    }).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        AppError::BadRequest(msg) => {
            assert!(msg.contains("Name"), "Expected 'Name' in message, got: {}", msg);
        }
        other => panic!("Expected BadRequest, got {:?}", other),
    }
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_create_empty_platform_rejected(pool: PgPool) {
    let admin = super_admin();

    let result = channel::create(&pool, &admin, CreateChannelRequest {
        platform: "".to_string(),
        name: "Valid Name".to_string(),
        credentials: serde_json::Value::Null,
        settings: serde_json::Value::Null,
        enabled: true,
    }).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        AppError::BadRequest(msg) => {
            assert!(msg.contains("Platform"), "Expected 'Platform' in message, got: {}", msg);
        }
        other => panic!("Expected BadRequest, got {:?}", other),
    }
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_update_own_channel(pool: PgPool) {
    let t1 = helpers::seed::seed_tenant(&pool, "Tenant Upd", "tenant-upd").await;
    let member_user = member(t1);

    let created = channel::create(&pool, &member_user, CreateChannelRequest {
        platform: "slack".to_string(),
        name: "Original".to_string(),
        credentials: serde_json::Value::Null,
        settings: serde_json::Value::Null,
        enabled: true,
    }).await.unwrap();

    let updated = channel::update(&pool, &member_user, created.id, UpdateChannelRequest {
        platform: None,
        name: Some("Renamed".to_string()),
        credentials: None,
        settings: None,
        enabled: Some(false),
    }).await.unwrap();

    assert_eq!(updated.id, created.id);
    assert_eq!(updated.name, "Renamed");
    assert_eq!(updated.platform, "slack"); // unchanged
    assert!(!updated.enabled);
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_update_other_tenant_forbidden(pool: PgPool) {
    let t1 = helpers::seed::seed_tenant(&pool, "Owner Tenant", "owner").await;
    let t2 = helpers::seed::seed_tenant(&pool, "Other Tenant", "other").await;

    let channel_id = helpers::seed::seed_channel(&pool, "Private", "slack", Some(t1)).await;

    let other_member = member(t2);
    let result = channel::update(&pool, &other_member, channel_id, UpdateChannelRequest {
        platform: None,
        name: Some("Hacked".to_string()),
        credentials: None,
        settings: None,
        enabled: None,
    }).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        AppError::Forbidden(_) => {}
        other => panic!("Expected Forbidden, got {:?}", other),
    }

    // Verify data unchanged in DB
    let unchanged = sqlx::query_as::<_, ops::models::channel::Channel>(
        "SELECT * FROM channels WHERE id = $1",
    )
    .bind(channel_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(unchanged.name, "Private");
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_update_not_found(pool: PgPool) {
    let admin = super_admin();

    let result = channel::update(&pool, &admin, Uuid::new_v4(), UpdateChannelRequest {
        platform: None,
        name: Some("Ghost".to_string()),
        credentials: None,
        settings: None,
        enabled: None,
    }).await;

    assert!(matches!(result, Err(AppError::NotFound(_))));
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_delete_success(pool: PgPool) {
    let t1 = helpers::seed::seed_tenant(&pool, "Del Tenant", "del-tenant").await;
    let admin = super_admin();

    let channel_id = helpers::seed::seed_channel(&pool, "To Delete", "teams", Some(t1)).await;

    channel::delete(&pool, &admin, channel_id).await.unwrap();

    // Verify it's gone by listing
    let channels = channel::list(&pool, &admin).await.unwrap();
    assert!(channels.iter().all(|c| c.id != channel_id));
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_delete_other_tenant_forbidden(pool: PgPool) {
    let t1 = helpers::seed::seed_tenant(&pool, "Keeper", "keeper").await;
    let t2 = helpers::seed::seed_tenant(&pool, "Intruder", "intruder").await;

    let channel_id = helpers::seed::seed_channel(&pool, "Protected", "slack", Some(t1)).await;

    let intruder = member(t2);
    let result = channel::delete(&pool, &intruder, channel_id).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        AppError::Forbidden(_) => {}
        other => panic!("Expected Forbidden, got {:?}", other),
    }
}
