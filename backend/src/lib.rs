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
    /// Incident timeline fan-out bus (W4). Every writer that inserts into
    /// `incident_timeline_events` also publishes onto this bus so SSE
    /// subscribers on `/api/incidents/:id/timeline/stream` see events in
    /// real time.
    pub timeline_bus: std::sync::Arc<services::incident::timeline_bus::TimelineBus>,
    /// Shared `reqwest::Client` with sane defaults (connect + total timeout,
    /// pooled idle connections). Build once in `main`, clone cheaply — every
    /// outbound HTTP call site should borrow this instead of constructing a
    /// fresh `Client::new()` per request, which would leak connection pools
    /// and bypass timeouts.
    pub http_client: reqwest::Client,
    /// Runtime toggle: whether auto RCA analysis fires on new issues.
    /// Persisted only in memory — survives hot-reload but resets on restart.
    /// Modifiable via `PUT /api/issues/rca/config` (super_admin only).
    pub auto_rca_enabled: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Broadcast channel for real-time notification push to SSE subscribers.
    /// Senders publish Notification structs; the SSE stream handler subscribes
    /// and filters by user_id before forwarding to the client.
    pub notification_tx: tokio::sync::broadcast::Sender<models::notification::Notification>,
}
