mod helpers;

use sqlx::PgPool;
use ops::middleware::auth::AuthUser;
use ops::services::telemetry;
use ops::models::telemetry::{CreateTelemetryRequest, UpdateTelemetryRequest};
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

fn make_req(name: &str) -> CreateTelemetryRequest {
    CreateTelemetryRequest {
        name: name.to_string(),
        provider: "grafana".to_string(),
        config: serde_json::json!({"url": "http://grafana.local"}),
        routing: serde_json::json!({"signals": ["metrics"], "scope": "all"}),
        enabled: true,
    }
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_list_empty(pool: PgPool) {
    let admin = super_admin();
    let result = telemetry::list(&pool, &admin).await.unwrap();
    assert!(result.is_empty());
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_create_and_list(pool: PgPool) {
    let tid = seed_tenant(&pool).await;
    let admin = admin_with_tenant(tid);

    let created = telemetry::create(&pool, &admin, make_req("prod-grafana")).await.unwrap();
    assert_eq!(created.name, "prod-grafana");
    assert_eq!(created.provider, "grafana");
    assert!(created.enabled);
    assert_eq!(created.tenant_id, Some(tid));

    let all = telemetry::list(&pool, &admin).await.unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].id, created.id);
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_create_empty_name_rejected(pool: PgPool) {
    let tid = seed_tenant(&pool).await;
    let admin = admin_with_tenant(tid);

    let result = telemetry::create(&pool, &admin, make_req("")).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        AppError::BadRequest(msg) => assert!(msg.contains("Name"), "Expected 'Name' in message, got: {}", msg),
        other => panic!("Expected BadRequest, got {:?}", other),
    }
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_update_success(pool: PgPool) {
    let tid = seed_tenant(&pool).await;
    let admin = admin_with_tenant(tid);

    let created = telemetry::create(&pool, &admin, make_req("original")).await.unwrap();

    let updated = telemetry::update(&pool, &admin, created.id, UpdateTelemetryRequest {
        name: Some("renamed".to_string()),
        provider: None,
        config: None,
        routing: None,
        enabled: None,
    }).await.unwrap();

    assert_eq!(updated.name, "renamed");
    assert_eq!(updated.provider, "grafana"); // unchanged
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_update_other_tenant_forbidden(pool: PgPool) {
    let tid1 = seed_tenant(&pool).await;
    let admin1 = admin_with_tenant(tid1);

    let created = telemetry::create(&pool, &admin1, make_req("tenant1-config")).await.unwrap();

    // Seed a second tenant
    let tid2: Uuid = sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ('t2', 't2') RETURNING id")
        .fetch_one(&pool)
        .await
        .unwrap();
    let member2 = member(tid2);

    let result = telemetry::update(&pool, &member2, created.id, UpdateTelemetryRequest {
        name: Some("hacked".to_string()),
        provider: None,
        config: None,
        routing: None,
        enabled: None,
    }).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        AppError::Forbidden(_) => {}
        other => panic!("Expected Forbidden, got {:?}", other),
    }
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_delete_success(pool: PgPool) {
    let tid = seed_tenant(&pool).await;
    let admin = admin_with_tenant(tid);

    let created = telemetry::create(&pool, &admin, make_req("to-delete")).await.unwrap();
    telemetry::delete(&pool, &admin, created.id).await.unwrap();

    let all = telemetry::list(&pool, &admin).await.unwrap();
    assert!(all.is_empty());
}
