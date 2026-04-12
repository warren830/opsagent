use sqlx::PgPool;
use uuid::Uuid;

/// Seed a tenant and return its ID.
pub async fn seed_tenant(pool: &PgPool, name: &str, slug: &str) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id",
    )
    .bind(name)
    .bind(slug)
    .fetch_one(pool)
    .await
    .expect("Failed to seed tenant")
}

/// Seed a channel and return its ID.
pub async fn seed_channel(pool: &PgPool, name: &str, platform: &str, tenant_id: Option<Uuid>) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO channels (platform, name, tenant_id) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(platform)
    .bind(name)
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .expect("Failed to seed channel")
}
