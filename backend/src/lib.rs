pub mod config;
pub mod db;
pub mod error;
pub mod handlers;
pub mod middleware;
pub mod models;
pub mod services;

pub use error::{AppError, AppResult};

/// Shared application state — accessible by all handlers via `State<AppState>`
#[derive(Clone)]
pub struct AppState {
    pub pool: sqlx::PgPool,
    pub config: config::AppConfig,
    pub rca_registry: std::sync::Arc<services::rca::RcaRegistry>,
}
