use sqlx::postgres::{PgPool, PgPoolOptions};
use std::time::Duration;

use crate::config::AppConfig;

/// Create a PostgreSQL connection pool with production-ready settings.
///
/// Addresses two common issues:
/// 1. Peak traffic errors → adequate pool size (max_connections)
/// 2. First request after idle fails → keepalive + test_before_acquire
pub async fn create_pool(config: &AppConfig) -> Result<PgPool, sqlx::Error> {
    let pool = PgPoolOptions::new()
        .max_connections(config.db_max_connections)
        .min_connections(config.db_min_connections)
        .acquire_timeout(Duration::from_secs(5))
        .idle_timeout(Duration::from_secs(600))
        .max_lifetime(Duration::from_secs(1800))
        .test_before_acquire(true)
        .connect(&config.database_url)
        .await?;

    tracing::info!(
        "Database pool created: max={}, min={}, url={}",
        config.db_max_connections,
        config.db_min_connections,
        mask_url(&config.database_url),
    );

    Ok(pool)
}

/// Run pending database migrations.
pub async fn run_migrations(pool: &PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    tracing::info!("Running database migrations...");
    sqlx::migrate!("./src/migrations").run(pool).await?;
    tracing::info!("Database migrations complete");
    Ok(())
}

/// Check database health.
pub async fn is_healthy(pool: &PgPool) -> bool {
    sqlx::query("SELECT 1").execute(pool).await.is_ok()
}

/// Mask password in database URL for logging.
fn mask_url(url: &str) -> String {
    if let Some(at_pos) = url.find('@')
        && let Some(colon_pos) = url[..at_pos].rfind(':')
    {
        let prefix = &url[..colon_pos + 1];
        let suffix = &url[at_pos..];
        return format!("{}****{}", prefix, suffix);
    }
    url.to_string()
}
