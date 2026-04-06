use http::Method;
use tower_http::cors::{AllowOrigin, CorsLayer};

use crate::config::AppConfig;

/// Build environment-aware CORS layer.
///
/// - local: Allow all origins (development convenience)
/// - dev/prod: Only allow configured origins (strict)
///
/// Reference: KBP 3-layer CORS strategy
pub fn build_cors_layer(config: &AppConfig) -> CorsLayer {
    let allow_origin = if config.env.is_local() && config.allowed_origins.is_empty() {
        // Local mode without explicit origins: allow localhost without credentials conflict
        tracing::warn!("CORS: Allowing localhost:3000 (local mode, no origins configured)");
        let origins: Vec<_> = vec!["http://localhost:3000".parse().unwrap()];
        AllowOrigin::list(origins)
    } else {
        let origins: Vec<_> = config.allowed_origins.iter().filter_map(|o| o.parse().ok()).collect();
        tracing::info!("CORS: Allowed origins: {:?}", config.allowed_origins);
        AllowOrigin::list(origins)
    };

    CorsLayer::new()
        .allow_origin(allow_origin)
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE, Method::OPTIONS])
        .allow_headers([
            http::header::CONTENT_TYPE,
            http::header::AUTHORIZATION,
            http::header::COOKIE,
            http::header::HeaderName::from_static("x-requested-with"),
        ])
        .allow_credentials(true)
        .max_age(std::time::Duration::from_secs(3600))
}
