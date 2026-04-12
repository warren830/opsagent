use sqlx::PgPool;
use uuid::Uuid;

use ops::error::AppError;
use ops::middleware::auth::AuthUser;
use ops::models::pipeline::CreatePipelineRepoRequest;
use ops::services::pipeline;

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
    let repos = pipeline::list(&pool, &admin).await.unwrap();
    assert!(repos.is_empty());
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_create_and_list(pool: PgPool) {
    let tid = seed_tenant(&pool).await;
    let user = member(tid);

    let repo = pipeline::create(
        &pool,
        &user,
        CreatePipelineRepoRequest {
            repo_id: "repo-1".to_string(),
            name: "My Repo".to_string(),
            repository: "https://github.com/example/repo.git".to_string(),
            token_secret_arn: None,
            description: Some("test desc".to_string()),
            enabled: true,
        },
    )
    .await
    .unwrap();

    assert_eq!(repo.name, "My Repo");
    assert_eq!(repo.tenant_id, Some(tid));

    let repos = pipeline::list(&pool, &user).await.unwrap();
    assert_eq!(repos.len(), 1);
    assert_eq!(repos[0].id, repo.id);
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_create_empty_name_rejected(pool: PgPool) {
    let tid = seed_tenant(&pool).await;
    let user = member(tid);

    let result = pipeline::create(
        &pool,
        &user,
        CreatePipelineRepoRequest {
            repo_id: "r".to_string(),
            name: "".to_string(),
            repository: "https://github.com/x/y.git".to_string(),
            token_secret_arn: None,
            description: None,
            enabled: true,
        },
    )
    .await;

    assert!(matches!(result, Err(AppError::BadRequest(_))));
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_create_empty_repository_rejected(pool: PgPool) {
    let tid = seed_tenant(&pool).await;
    let user = member(tid);

    let result = pipeline::create(
        &pool,
        &user,
        CreatePipelineRepoRequest {
            repo_id: "r".to_string(),
            name: "Valid Name".to_string(),
            repository: "".to_string(),
            token_secret_arn: None,
            description: None,
            enabled: true,
        },
    )
    .await;

    assert!(matches!(result, Err(AppError::BadRequest(_))));
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_update_own_tenant(pool: PgPool) {
    let tid = seed_tenant(&pool).await;
    let user = member(tid);

    let repo = pipeline::create(
        &pool,
        &user,
        CreatePipelineRepoRequest {
            repo_id: "r1".to_string(),
            name: "Original".to_string(),
            repository: "https://github.com/x/y.git".to_string(),
            token_secret_arn: None,
            description: None,
            enabled: true,
        },
    )
    .await
    .unwrap();

    let updated = pipeline::update(
        &pool,
        &user,
        repo.id,
        ops::models::pipeline::UpdatePipelineRepoRequest {
            name: Some("Updated".to_string()),
            repository: None,
            token_secret_arn: None,
            description: None,
            enabled: None,
        },
    )
    .await
    .unwrap();

    assert_eq!(updated.name, "Updated");
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_update_other_tenant_forbidden(pool: PgPool) {
    let tid1 = seed_tenant(&pool).await;
    let user1 = member(tid1);

    let repo = pipeline::create(
        &pool,
        &user1,
        CreatePipelineRepoRequest {
            repo_id: "r1".to_string(),
            name: "Owner Repo".to_string(),
            repository: "https://github.com/x/y.git".to_string(),
            token_secret_arn: None,
            description: None,
            enabled: true,
        },
    )
    .await
    .unwrap();

    // Different tenant member tries to update
    let tid2: Uuid = sqlx::query_scalar(
        "INSERT INTO tenants (name, slug) VALUES ('t2', 't2') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let user2 = member(tid2);

    let result = pipeline::update(
        &pool,
        &user2,
        repo.id,
        ops::models::pipeline::UpdatePipelineRepoRequest {
            name: Some("Hacked".to_string()),
            repository: None,
            token_secret_arn: None,
            description: None,
            enabled: None,
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
async fn test_delete_success(pool: PgPool) {
    let tid = seed_tenant(&pool).await;
    let user = member(tid);

    let repo = pipeline::create(
        &pool,
        &user,
        CreatePipelineRepoRequest {
            repo_id: "r1".to_string(),
            name: "Delete Me".to_string(),
            repository: "https://github.com/x/y.git".to_string(),
            token_secret_arn: None,
            description: None,
            enabled: true,
        },
    )
    .await
    .unwrap();

    pipeline::delete(&pool, &user, repo.id).await.unwrap();

    let repos = pipeline::list(&pool, &user).await.unwrap();
    assert!(repos.is_empty());
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_update_not_found(pool: PgPool) {
    let admin = super_admin();

    let result = pipeline::update(
        &pool,
        &admin,
        Uuid::new_v4(),
        ops::models::pipeline::UpdatePipelineRepoRequest {
            name: Some("Ghost".to_string()),
            repository: None,
            token_secret_arn: None,
            description: None,
            enabled: None,
        },
    )
    .await;

    assert!(matches!(result, Err(AppError::NotFound(_))));
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_delete_not_found(pool: PgPool) {
    let admin = super_admin();

    let result = pipeline::delete(&pool, &admin, Uuid::new_v4()).await;

    assert!(matches!(result, Err(AppError::NotFound(_))));
}
