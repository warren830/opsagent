use axum::{Json, extract::State};
use serde_json::{Value, json};

use crate::AppState;

/// GET /health — health check with dependency status
pub async fn health_check(State(state): State<AppState>) -> Json<Value> {
    let db_healthy = crate::db::is_healthy(&state.pool).await;

    let status = if db_healthy { "ok" } else { "degraded" };

    Json(json!({
        "status": status,
        "version": env!("CARGO_PKG_VERSION"),
        "dependencies": {
            "database": if db_healthy { "ok" } else { "error" },
        }
    }))
}
