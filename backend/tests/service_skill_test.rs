use sqlx::PgPool;
use uuid::Uuid;

use ops::middleware::auth::AuthUser;
use ops::services::skill;

fn super_admin() -> AuthUser {
    AuthUser {
        user_id: Uuid::new_v4(),
        role: "super_admin".to_string(),
        tenant_id: None,
        username: "admin".to_string(),
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
    let skills = skill::list(&pool, &admin).await.unwrap();
    assert!(skills.is_empty());
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_list_super_admin_sees_all(pool: PgPool) {
    let tid = seed_tenant(&pool).await;

    // Seed skills directly via SQL
    sqlx::query(
        "INSERT INTO skills (name, visibility, enabled, tenant_id) VALUES ($1, $2, true, $3)",
    )
    .bind("test-skill-a")
    .bind("public")
    .bind(tid)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO skills (name, visibility, enabled, tenant_id) VALUES ($1, $2, true, $3)",
    )
    .bind("test-skill-b")
    .bind("private")
    .bind(tid)
    .execute(&pool)
    .await
    .unwrap();

    // Super admin sees all skills
    let admin = super_admin();
    let skills = skill::list(&pool, &admin).await.unwrap();
    assert_eq!(skills.len(), 2);

    let names: Vec<&str> = skills.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"test-skill-a"));
    assert!(names.contains(&"test-skill-b"));
}
