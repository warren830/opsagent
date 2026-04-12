use sqlx::PgPool;
use uuid::Uuid;

use openops::error::AppError;
use openops::middleware::auth::AuthUser;
use openops::models::scheduled_job::CreateScheduledJobRequest;
use openops::services::scheduled_job;

fn member_with_id(user_id: Uuid, tenant_id: Uuid) -> AuthUser {
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

/// Seed a user in the users table and return an AuthUser with matching user_id.
async fn seed_member(pool: &PgPool, tenant_id: Uuid, username: &str) -> AuthUser {
    let user_id: Uuid = sqlx::query_scalar(
        "INSERT INTO users (username, password_hash, role, tenant_id) VALUES ($1, 'hash', 'member', $2) RETURNING id",
    )
    .bind(username)
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .unwrap();
    member_with_id(user_id, tenant_id)
}

fn make_req(name: &str, cron: &str) -> CreateScheduledJobRequest {
    CreateScheduledJobRequest {
        name: name.to_string(),
        cron_expression: cron.to_string(),
        timezone: "UTC".to_string(),
        query: Some("check pods".to_string()),
        enabled: true,
        auto_jira: false,
        targets: serde_json::json!({}),
        visibility: "public".to_string(),
        job_type: "agent".to_string(),
        skill_path: None,
        skill_params: serde_json::Value::Null,
    }
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_create_and_list(pool: PgPool) {
    let tid = seed_tenant(&pool).await;
    let user = seed_member(&pool, tid, "user1").await;

    let job = scheduled_job::create(&pool, &user, make_req("daily-check", "0 0 * * *"))
        .await
        .unwrap();

    assert_eq!(job.name, "daily-check");
    assert_eq!(job.tenant_id, Some(tid));

    let jobs = scheduled_job::list(&pool, &user).await.unwrap();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].id, job.id);
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_create_empty_name_rejected(pool: PgPool) {
    let tid = seed_tenant(&pool).await;
    let user = seed_member(&pool, tid, "u2").await;

    let result = scheduled_job::create(&pool, &user, make_req("", "0 0 * * *")).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        AppError::BadRequest(msg) => assert!(msg.contains("Name"), "got: {}", msg),
        other => panic!("Expected BadRequest, got {:?}", other),
    }
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_create_empty_cron_rejected(pool: PgPool) {
    let tid = seed_tenant(&pool).await;
    let user = seed_member(&pool, tid, "u3").await;

    let result = scheduled_job::create(&pool, &user, make_req("ok-name", "")).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        AppError::BadRequest(msg) => assert!(msg.contains("Cron"), "got: {}", msg),
        other => panic!("Expected BadRequest, got {:?}", other),
    }
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_create_skill_type_requires_path(pool: PgPool) {
    let tid = seed_tenant(&pool).await;
    let user = seed_member(&pool, tid, "u4").await;

    let mut req = make_req("skill-job", "0 0 * * *");
    req.job_type = "skill".to_string();
    req.skill_path = None;

    let result = scheduled_job::create(&pool, &user, req).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        AppError::BadRequest(msg) => assert!(msg.contains("Skill path"), "got: {}", msg),
        other => panic!("Expected BadRequest, got {:?}", other),
    }
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_private_job_only_visible_to_owner(pool: PgPool) {
    let tid = seed_tenant(&pool).await;
    let owner = seed_member(&pool, tid, "owner1").await;
    let owner_id = owner.user_id;

    let mut req = make_req("my-private-job", "0 0 * * *");
    req.visibility = "private".to_string();

    let job = scheduled_job::create(&pool, &owner, req).await.unwrap();
    assert_eq!(job.user_id, Some(owner_id));
    assert_eq!(job.visibility, "private");

    // Owner sees the job
    let owner_jobs = scheduled_job::list(&pool, &owner).await.unwrap();
    assert_eq!(owner_jobs.len(), 1);

    // Different member of same tenant cannot see it
    let other = seed_member(&pool, tid, "other1").await;
    let other_jobs = scheduled_job::list(&pool, &other).await.unwrap();
    assert!(other_jobs.is_empty());
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_delete_other_user_forbidden(pool: PgPool) {
    let tid = seed_tenant(&pool).await;
    let owner = seed_member(&pool, tid, "owner2").await;

    let mut req = make_req("private-job", "0 0 * * *");
    req.visibility = "private".to_string();

    let job = scheduled_job::create(&pool, &owner, req).await.unwrap();

    // Other member cannot delete
    let other = seed_member(&pool, tid, "other2").await;
    let result = scheduled_job::delete(&pool, &other, job.id).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        AppError::Forbidden(_) => {}
        other => panic!("Expected Forbidden, got {:?}", other),
    }
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_trigger_run_creates_pending_record(pool: PgPool) {
    let tid = seed_tenant(&pool).await;
    let user = seed_member(&pool, tid, "u5").await;

    let job = scheduled_job::create(&pool, &user, make_req("trigger-test", "0 0 * * *"))
        .await
        .unwrap();

    let (run, returned_job) = scheduled_job::trigger_run(&pool, &user, job.id)
        .await
        .unwrap();

    assert_eq!(run.job_id, job.id);
    assert_eq!(run.status, "pending");
    assert_eq!(run.trigger, "manual");
    assert_eq!(returned_job.id, job.id);
}
