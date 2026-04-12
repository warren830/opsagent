mod helpers;

use sqlx::PgPool;
use openops::middleware::auth::AuthUser;
use openops::services::dashboard;
use uuid::Uuid;

fn super_admin() -> AuthUser {
    AuthUser {
        user_id: Uuid::new_v4(),
        role: "super_admin".to_string(),
        tenant_id: None,
        username: "admin".to_string(),
    }
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_stats_empty_db(pool: PgPool) {
    let admin = super_admin();
    let stats = dashboard::stats(&pool, &admin).await.unwrap();
    assert_eq!(stats.tenants, 0);
    assert_eq!(stats.users, 0);
    assert_eq!(stats.skills, 0);
    assert_eq!(stats.clusters, 0);
    assert_eq!(stats.issues_open, 0);
    assert_eq!(stats.active_sessions, 0);
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_stats_super_admin_counts_all(pool: PgPool) {
    // Seed a tenant
    let tid: Uuid = sqlx::query_scalar(
        "INSERT INTO tenants (name, slug) VALUES ('Acme', 'acme') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    // Seed a user (password_hash is required)
    sqlx::query(
        "INSERT INTO users (username, password_hash, role, tenant_id) VALUES ('bob', '$2b$12$dummy', 'member', $1)",
    )
    .bind(tid)
    .execute(&pool)
    .await
    .unwrap();

    // Seed a skill (no user_id -> tenant-wide)
    sqlx::query(
        "INSERT INTO skills (name, description, tenant_id) VALUES ('deploy', 'deploy skill', $1)",
    )
    .bind(tid)
    .execute(&pool)
    .await
    .unwrap();

    // Seed a cluster
    sqlx::query(
        "INSERT INTO clusters (name, cloud, cluster_type, tenant_id) VALUES ('prod', 'aws', 'eks', $1)",
    )
    .bind(tid)
    .execute(&pool)
    .await
    .unwrap();

    let admin = super_admin();
    let stats = dashboard::stats(&pool, &admin).await.unwrap();
    assert_eq!(stats.tenants, 1);
    assert_eq!(stats.users, 1);
    assert_eq!(stats.skills, 1);
    assert_eq!(stats.clusters, 1);
    assert_eq!(stats.issues_open, 0);
    assert_eq!(stats.active_sessions, 0);
}
