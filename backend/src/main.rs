mod config;
mod db;
mod error;
mod handlers;
mod middleware;
mod models;
mod services;

use axum::{
    Router, middleware as axum_middleware,
    routing::{delete, get, post, put},
};
use std::net::SocketAddr;
use tokio::signal;
use tower_http::{limit::RequestBodyLimitLayer, trace::TraceLayer};

use crate::config::AppConfig;

/// Shared application state — accessible by all handlers via `State<AppState>`
#[derive(Clone)]
pub struct AppState {
    pub pool: sqlx::PgPool,
    pub config: AppConfig,
}

#[tokio::main]
async fn main() {
    // Load .env file (ignore errors if not present)
    let _ = dotenvy::dotenv();

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "openops=debug,tower_http=debug".into()),
        )
        .init();

    // Load config
    let config = AppConfig::from_env();
    tracing::info!("Starting OpenOps backend (env={:?})", config.env);

    // Create database pool
    let pool = db::create_pool(&config).await.expect("Failed to create database pool");

    // Run migrations
    db::run_migrations(&pool)
        .await
        .expect("Failed to run database migrations");

    // Seed default admin user if no users exist
    seed_admin_user(&pool).await;

    // Build app state
    let state = AppState {
        pool,
        config: config.clone(),
    };

    // Build CORS layer
    let cors = middleware::cors::build_cors_layer(&config);

    // Build router
    let app = build_router(state)
        .layer(cors)
        .layer(axum_middleware::from_fn(middleware::security::security_headers))
        .layer(TraceLayer::new_for_http())
        .layer(RequestBodyLimitLayer::new(20 * 1024 * 1024)); // 20MB (images can be large)

    // Start server
    let addr = SocketAddr::from(([0, 0, 0, 0], config.backend_port));
    tracing::info!("Listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("Failed to bind address");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("Server error");

    tracing::info!("Server shut down gracefully");
}

fn build_router(state: AppState) -> Router {
    let jwt_secret = state.config.jwt_secret.clone();

    // Public routes (no auth required)
    let public_routes = Router::new()
        .route("/health", get(handlers::health::health_check))
        .route("/api/auth/login", post(handlers::auth::login));

    // Protected routes (auth required)
    let protected_routes = Router::new()
        .route("/api/auth/logout", post(handlers::auth::logout))
        .route("/api/auth/me", get(handlers::auth::me))
        .route("/api/auth/change-password", put(handlers::auth::change_password))
        // Tenants
        .route("/api/tenants", get(handlers::tenant::list_tenants))
        .route("/api/tenants", post(handlers::tenant::create_tenant))
        .route("/api/tenants/{id}", get(handlers::tenant::get_tenant))
        .route("/api/tenants/{id}", put(handlers::tenant::update_tenant))
        .route("/api/tenants/{id}", delete(handlers::tenant::delete_tenant))
        // Users
        .route("/api/users", get(handlers::user::list_users))
        .route("/api/users", post(handlers::user::create_user))
        .route("/api/users/{id}", put(handlers::user::update_user))
        .route("/api/users/{id}", delete(handlers::user::delete_user))
        // Glossary
        .route(
            "/api/glossary",
            get(handlers::glossary::list).post(handlers::glossary::create),
        )
        .route(
            "/api/glossary/{id}",
            put(handlers::glossary::update).delete(handlers::glossary::delete),
        )
        // Skills (DB + git clone)
        .route("/api/skills", get(handlers::skill::list).post(handlers::skill::create))
        .route("/api/skills/discover", post(handlers::skill::discover))
        .route(
            "/api/skills/{id}",
            put(handlers::skill::update).delete(handlers::skill::delete),
        )
        // Cloud Accounts
        .route(
            "/api/accounts",
            get(handlers::cloud_account::list).post(handlers::cloud_account::create),
        )
        .route(
            "/api/accounts/{id}",
            put(handlers::cloud_account::update).delete(handlers::cloud_account::delete),
        )
        // Account Access Control
        .route(
            "/api/accounts/{id}/users",
            get(handlers::account_access::list_account_users),
        )
        .route("/api/account-access/grant", post(handlers::account_access::grant))
        .route(
            "/api/account-access/{user_id}/{account_id}",
            delete(handlers::account_access::revoke),
        )
        .route(
            "/api/my/accessible-accounts",
            get(handlers::account_access::my_accessible_accounts),
        )
        .route("/api/accounts/discover", post(handlers::cloud_account::discover))
        .route(
            "/api/accounts/{id}/test",
            post(handlers::cloud_account::test_connection),
        )
        .route("/api/accounts/seed-mock", post(handlers::cloud_account::seed_mock))
        // Approvals
        .route("/api/approvals", get(handlers::approval::list))
        .route("/api/approvals/{id}/approve", post(handlers::approval::approve))
        .route("/api/approvals/{id}/reject", post(handlers::approval::reject))
        // Channels
        .route(
            "/api/channels",
            get(handlers::channel::list).post(handlers::channel::create),
        )
        .route(
            "/api/channels/{id}",
            put(handlers::channel::update).delete(handlers::channel::delete),
        )
        // Clusters
        .route(
            "/api/clusters",
            get(handlers::cluster::list).post(handlers::cluster::create),
        )
        .route(
            "/api/clusters/{id}",
            put(handlers::cluster::update).delete(handlers::cluster::delete),
        )
        // Resources
        .route("/api/resources", get(handlers::resource::list))
        .route("/api/resources/scan", post(handlers::resource::scan))
        // Issues
        .route("/api/issues", get(handlers::issue::list))
        .route(
            "/api/issues/{id}",
            get(handlers::issue::get).put(handlers::issue::update),
        )
        .route("/api/issues/{id}/rca", post(handlers::issue::start_rca))
        // Knowledge
        .route(
            "/api/knowledge",
            get(handlers::knowledge::list).post(handlers::knowledge::create),
        )
        .route(
            "/api/knowledge/{id}",
            put(handlers::knowledge::update).delete(handlers::knowledge::delete),
        )
        // Scheduled Jobs
        .route(
            "/api/scheduled-jobs",
            get(handlers::scheduled_job::list).post(handlers::scheduled_job::create),
        )
        .route(
            "/api/scheduled-jobs/{id}",
            put(handlers::scheduled_job::update).delete(handlers::scheduled_job::delete),
        )
        // Pipeline Repos
        .route(
            "/api/pipeline/repos",
            get(handlers::pipeline::list).post(handlers::pipeline::create),
        )
        .route(
            "/api/pipeline/repos/{id}",
            put(handlers::pipeline::update).delete(handlers::pipeline::delete),
        )
        // Telemetry
        .route(
            "/api/telemetry",
            get(handlers::telemetry::get).put(handlers::telemetry::upsert),
        )
        .route("/api/telemetry/test", post(handlers::telemetry::test_connection))
        // Providers (LLM model config)
        .route(
            "/api/providers",
            get(handlers::provider::list).post(handlers::provider::create),
        )
        .route("/api/providers/types", get(handlers::provider::available_types))
        .route(
            "/api/providers/{id}",
            put(handlers::provider::update).delete(handlers::provider::delete),
        )
        // MCP Servers
        .route("/api/mcp", get(handlers::mcp::list).post(handlers::mcp::create))
        .route(
            "/api/mcp/{id}",
            put(handlers::mcp::update).delete(handlers::mcp::delete),
        )
        // Chat (SSE streaming)
        .route("/api/chat", post(handlers::chat::stream))
        .route("/api/chat/sessions", get(handlers::chat::list_sessions))
        .route("/api/chat/workspace", get(handlers::chat::workspace_list))
        .route(
            "/api/chat/workspace/{*filepath}",
            get(handlers::chat::workspace_download).delete(handlers::chat::workspace_delete),
        )
        // Dashboard
        .route("/api/dashboard/stats", get(handlers::dashboard::stats))
        .layer(axum_middleware::from_fn_with_state(
            jwt_secret,
            middleware::auth::auth_middleware,
        ));

    public_routes.merge(protected_routes).with_state(state)
}

/// Seed a default admin user if no users exist in the database
async fn seed_admin_user(pool: &sqlx::PgPool) {
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await
        .unwrap_or((0,));

    if count.0 == 0 {
        let password_hash = bcrypt::hash("admin123", 10).expect("Failed to hash default password");

        let result = sqlx::query(
            r#"INSERT INTO users (username, password_hash, role, email)
               VALUES ('admin', $1, 'super_admin', 'admin@openops.local')"#,
        )
        .bind(&password_hash)
        .execute(pool)
        .await;

        match result {
            Ok(_) => {
                tracing::info!("Default admin user created (username: admin, password: admin123)");
                tracing::warn!("Change the default password immediately!");
            }
            Err(e) => {
                tracing::warn!("Failed to seed admin user: {}", e);
            }
        }
    }
}

/// Graceful shutdown signal handler
async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c().await.expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => tracing::info!("Received Ctrl+C, shutting down..."),
        _ = terminate => tracing::info!("Received SIGTERM, shutting down..."),
    }
}
