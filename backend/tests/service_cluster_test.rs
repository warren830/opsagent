use sqlx::PgPool;
use uuid::Uuid;

use ops::error::AppError;
use ops::middleware::auth::AuthUser;
use ops::models::cluster::{CreateClusterRequest, UpdateClusterRequest};
use ops::services::cluster;

fn super_admin() -> AuthUser {
    AuthUser {
        user_id: Uuid::new_v4(),
        role: "super_admin".to_string(),
        tenant_id: None,
        username: "admin".to_string(),
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

#[sqlx::test(migrations = "src/migrations")]
async fn test_list_empty(pool: PgPool) {
    let admin = super_admin();
    let clusters = cluster::list(&pool, &admin).await.unwrap();
    assert!(clusters.is_empty());
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_create_and_list(pool: PgPool) {
    let tid = seed_tenant(&pool).await;
    let user = member(tid);

    let c = cluster::create(
        &pool,
        &user,
        CreateClusterRequest {
            name: "prod-eks".to_string(),
            cloud: "aws".to_string(),
            cluster_type: "eks".to_string(),
            account_id: Some("123456789012".to_string()),
            region: Some("us-east-1".to_string()),
            role_name: None,
            description: Some("Production cluster".to_string()),
            config: serde_json::json!({}),
        },
    )
    .await
    .unwrap();

    assert_eq!(c.name, "prod-eks");
    assert_eq!(c.cloud, "aws");
    assert_eq!(c.tenant_id, Some(tid));

    let clusters = cluster::list(&pool, &user).await.unwrap();
    assert_eq!(clusters.len(), 1);
    assert_eq!(clusters[0].id, c.id);
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_create_empty_name_rejected(pool: PgPool) {
    let tid = seed_tenant(&pool).await;
    let user = member(tid);

    let result = cluster::create(
        &pool,
        &user,
        CreateClusterRequest {
            name: "".to_string(),
            cloud: "aws".to_string(),
            cluster_type: "eks".to_string(),
            account_id: None,
            region: None,
            role_name: None,
            description: None,
            config: serde_json::json!({}),
        },
    )
    .await;

    assert!(result.is_err());
    match result.unwrap_err() {
        AppError::BadRequest(_) => {}
        other => panic!("Expected BadRequest, got {:?}", other),
    }
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_update_own_tenant(pool: PgPool) {
    let tid = seed_tenant(&pool).await;
    let user = member(tid);

    let c = cluster::create(
        &pool,
        &user,
        CreateClusterRequest {
            name: "staging".to_string(),
            cloud: "aws".to_string(),
            cluster_type: "eks".to_string(),
            account_id: None,
            region: None,
            role_name: None,
            description: None,
            config: serde_json::json!({}),
        },
    )
    .await
    .unwrap();

    let updated = cluster::update(
        &pool,
        &user,
        c.id,
        UpdateClusterRequest {
            name: Some("staging-v2".to_string()),
            cloud: None,
            cluster_type: None,
            account_id: None,
            region: None,
            role_name: None,
            description: Some("Updated desc".to_string()),
            status: None,
            config: None,
        },
    )
    .await
    .unwrap();

    assert_eq!(updated.name, "staging-v2");
    assert_eq!(updated.description, Some("Updated desc".to_string()));
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_update_other_tenant_forbidden(pool: PgPool) {
    let tid1 = seed_tenant(&pool).await;
    let user1 = member(tid1);

    let c = cluster::create(
        &pool,
        &user1,
        CreateClusterRequest {
            name: "my-cluster".to_string(),
            cloud: "aws".to_string(),
            cluster_type: "eks".to_string(),
            account_id: None,
            region: None,
            role_name: None,
            description: None,
            config: serde_json::json!({}),
        },
    )
    .await
    .unwrap();

    let tid2: Uuid = sqlx::query_scalar(
        "INSERT INTO tenants (name, slug) VALUES ('t2', 't2') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let user2 = member(tid2);

    let result = cluster::update(
        &pool,
        &user2,
        c.id,
        UpdateClusterRequest {
            name: Some("hacked".to_string()),
            cloud: None,
            cluster_type: None,
            account_id: None,
            region: None,
            role_name: None,
            description: None,
            status: None,
            config: None,
        },
    )
    .await;

    assert!(result.is_err());
    match result.unwrap_err() {
        AppError::Forbidden(_) => {}
        other => panic!("Expected Forbidden, got {:?}", other),
    }

    // Verify the cluster name was NOT changed
    let unchanged = sqlx::query_scalar::<_, String>("SELECT name FROM clusters WHERE id = $1")
        .bind(c.id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(unchanged, "my-cluster");
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_delete_success(pool: PgPool) {
    let tid = seed_tenant(&pool).await;
    let user = member(tid);

    let c = cluster::create(
        &pool,
        &user,
        CreateClusterRequest {
            name: "delete-me".to_string(),
            cloud: "aws".to_string(),
            cluster_type: "eks".to_string(),
            account_id: None,
            region: None,
            role_name: None,
            description: None,
            config: serde_json::json!({}),
        },
    )
    .await
    .unwrap();

    cluster::delete(&pool, &user, c.id).await.unwrap();

    let clusters = cluster::list(&pool, &user).await.unwrap();
    assert!(clusters.is_empty());
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_delete_not_found(pool: PgPool) {
    let admin = super_admin();

    let result = cluster::delete(&pool, &admin, Uuid::new_v4()).await;

    assert!(matches!(result, Err(AppError::NotFound(_))));
}
