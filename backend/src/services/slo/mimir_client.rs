//! Mimir / Prometheus-compatible query proxy used by the SLO engine.
//!
//! This module is intentionally small — it handles the two shapes of
//! Prometheus HTTP API calls the W2 handlers need:
//!
//! * `instant` — `/api/v1/query` at a single point in time
//! * `range`   — `/api/v1/query_range` over a `[start, end]` window with step
//!
//! Endpoint and (optional) basic-auth credentials are discovered from the
//! `telemetry_config` table: we pick the first enabled row whose `routing`
//! includes the `"metrics"` signal. Multi-tenancy currently mirrors the
//! existing `prediction` service — no tenant filter on the lookup — because
//! telemetry configs are a shared platform resource. When that changes,
//! update [`resolve_metrics_endpoint`] rather than duplicating the SQL.
//!
//! The handlers call into this module; the module itself has no knowledge of
//! Axum or the HTTP response shape beyond returning the raw Prometheus
//! `data` envelope (already a `serde_json::Value`) for direct pass-through.

use reqwest::Client;
use serde_json::Value;
use sqlx::PgPool;
use std::sync::OnceLock;
use std::time::Duration;

use crate::error::{AppError, AppResult};

/// Process-wide fallback HTTP client used when a caller does not thread
/// `AppState::http_client` into these helpers. Built once on first use with
/// the same timeout + pool settings as the `main.rs` client. Having a single
/// shared fallback prevents `reqwest::Client::new()` from being called
/// per-request, which would leak connection pools.
static SHARED_CLIENT: OnceLock<Client> = OnceLock::new();

fn shared_client() -> &'static Client {
    SHARED_CLIENT.get_or_init(|| {
        Client::builder()
            .timeout(Duration::from_secs(15))
            .connect_timeout(Duration::from_secs(5))
            .pool_max_idle_per_host(10)
            .build()
            .unwrap_or_else(|_| Client::new())
    })
}

/// Resolved Mimir metrics backend: base URL + optional basic-auth pair used
/// when the telemetry provider is a cloud-managed Prometheus.
#[derive(Debug, Clone)]
pub struct MetricsEndpoint {
    pub url: String,
    /// `(user_id, api_token)` — populated when the telemetry provider is a
    /// cloud vendor (`provider != "self-hosted"`) and both fields are set on
    /// the config blob.
    pub basic_auth: Option<(String, String)>,
}

/// Look up the first enabled telemetry config that routes the `metrics`
/// signal and extract its Mimir endpoint.
///
/// Returns `AppError::BadRequest` with a user-visible message when no
/// metrics backend has been configured so the UI can render a helpful
/// empty-state instead of a 500.
pub async fn resolve_metrics_endpoint(pool: &PgPool) -> AppResult<MetricsEndpoint> {
    let row = sqlx::query_as::<_, (String, Value, bool)>(
        r#"SELECT provider, config, enabled FROM telemetry_config
           WHERE enabled = true
             AND routing->'signals' ? 'metrics'
           ORDER BY created_at DESC LIMIT 1"#,
    )
    .fetch_optional(pool)
    .await?;

    let (provider, config, _) = row.ok_or_else(|| {
        AppError::BadRequest(
            "No enabled telemetry config with metrics routing. Configure a Mimir or Prometheus backend first.".to_string(),
        )
    })?;

    let url = config
        .get("mimir_endpoint_url")
        .or_else(|| config.get("mimir_endpoint"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            AppError::BadRequest(
                "Telemetry config is missing `mimir_endpoint_url`.".to_string(),
            )
        })?;

    let basic_auth = if provider != "self-hosted" {
        let user = config.get("mimir_user_id").and_then(|v| v.as_str()).unwrap_or("");
        let token = config.get("api_token").and_then(|v| v.as_str()).unwrap_or("");
        if !user.is_empty() && !token.is_empty() {
            Some((user.to_string(), token.to_string()))
        } else {
            None
        }
    } else {
        None
    };

    Ok(MetricsEndpoint { url, basic_auth })
}

/// Fire a `/api/v1/query_range` request and return the parsed JSON body.
///
/// `query` is raw PromQL. `start` / `end` are UNIX seconds. `step` is a
/// Prometheus duration string (e.g. `"5m"`, `"1h"`). Uses the process-wide
/// `shared_client()` to avoid the per-request `Client::new()` that P1 #6
/// flagged as leaking connection pools and sidestepping timeouts.
pub async fn query_range(
    endpoint: &MetricsEndpoint,
    query: &str,
    start: i64,
    end: i64,
    step: &str,
) -> AppResult<Value> {
    let client = shared_client();
    let url = format!("{}/api/v1/query_range", endpoint.url.trim_end_matches('/'));

    let start_s = start.to_string();
    let end_s = end.to_string();
    let form = [
        ("query", query),
        ("start", start_s.as_str()),
        ("end", end_s.as_str()),
        ("step", step),
    ];

    let mut req = client.post(&url).form(&form);
    if let Some((user, token)) = &endpoint.basic_auth {
        req = req.basic_auth(user, Some(token));
    }

    let response = req
        .send()
        .await
        .map_err(|e| AppError::HttpClient(format!("Mimir query_range failed: {}", e)))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(AppError::HttpClient(format!(
            "Mimir query_range returned {}: {}",
            status, body
        )));
    }

    response
        .json::<Value>()
        .await
        .map_err(|e| AppError::HttpClient(format!("Mimir query_range bad JSON: {}", e)))
}

/// Fire a `/api/v1/query` instant request and return the parsed JSON body.
///
/// Used by the snapshot scheduler and other one-shot readers that don't need
/// a time series. `time` is UNIX seconds; pass `None` to let Mimir pick
/// "now". The returned JSON is the raw Prometheus `data` envelope. Uses the
/// process-wide `shared_client()` for the same reason as `query_range`.
pub async fn query_instant(
    endpoint: &MetricsEndpoint,
    query: &str,
    time: Option<i64>,
) -> AppResult<Value> {
    let client = shared_client();
    let url = format!("{}/api/v1/query", endpoint.url.trim_end_matches('/'));

    let time_s = time.map(|t| t.to_string());
    let mut form: Vec<(&str, &str)> = vec![("query", query)];
    if let Some(ref t) = time_s {
        form.push(("time", t.as_str()));
    }

    let mut req = client.post(&url).form(&form);
    if let Some((user, token)) = &endpoint.basic_auth {
        req = req.basic_auth(user, Some(token));
    }

    let response = req
        .send()
        .await
        .map_err(|e| AppError::HttpClient(format!("Mimir query_instant failed: {}", e)))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(AppError::HttpClient(format!(
            "Mimir query_instant returned {}: {}",
            status, body
        )));
    }

    response
        .json::<Value>()
        .await
        .map_err(|e| AppError::HttpClient(format!("Mimir query_instant bad JSON: {}", e)))
}

/// Extract the first scalar value from a Prometheus `query` (instant) response.
///
/// Prometheus wraps instant results as either `vector` (one sample per series)
/// or `scalar` (`[timestamp, "value"]`). Returns `None` for empty results or
/// unparseable values — callers should treat that as "no data yet".
pub fn first_scalar(result: &Value) -> Option<f64> {
    let data = result.get("data")?;
    let result_type = data.get("resultType")?.as_str()?;
    let payload = data.get("result")?;

    match result_type {
        "vector" => {
            // [{ metric: {...}, value: [ts, "val"] }, ...]
            let first = payload.as_array()?.first()?;
            let value = first.get("value")?.as_array()?;
            value.get(1)?.as_str()?.parse::<f64>().ok()
        }
        "scalar" => {
            // [ts, "val"]
            let arr = payload.as_array()?;
            arr.get(1)?.as_str()?.parse::<f64>().ok()
        }
        _ => None,
    }
}

/// Parse a Prometheus duration suffix (`s`, `m`, `h`, `d`) into seconds.
///
/// Used by the preview and SLI endpoints so handlers can accept strings like
/// `"28d"` without pulling in a full PromQL duration library. Returns
/// `AppError::BadRequest` when the input is malformed.
pub fn parse_duration_to_seconds(raw: &str) -> AppResult<i64> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(AppError::BadRequest("Duration is empty".to_string()));
    }
    let (num_part, unit) = trimmed.split_at(trimmed.len() - 1);
    let value: i64 = num_part.parse().map_err(|_| {
        AppError::BadRequest(format!("Invalid duration '{}': expected <number><unit>", raw))
    })?;
    if value <= 0 {
        return Err(AppError::BadRequest(format!(
            "Duration '{}' must be positive",
            raw
        )));
    }
    let seconds = match unit {
        "s" => value,
        "m" => value * 60,
        "h" => value * 3600,
        "d" => value * 86400,
        other => {
            return Err(AppError::BadRequest(format!(
                "Unsupported duration unit '{}' in '{}'; allowed: s, m, h, d",
                other, raw
            )));
        }
    };
    Ok(seconds)
}

// ---------------------------------------------------------------------------
// Tests — keep these I/O-free. Anything that would actually touch Mimir
// belongs in an integration test.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_duration_handles_all_supported_units() {
        assert_eq!(parse_duration_to_seconds("30s").unwrap(), 30);
        assert_eq!(parse_duration_to_seconds("5m").unwrap(), 300);
        assert_eq!(parse_duration_to_seconds("2h").unwrap(), 7200);
        assert_eq!(parse_duration_to_seconds("28d").unwrap(), 28 * 86400);
    }

    #[test]
    fn parse_duration_rejects_non_positive_or_bad_units() {
        assert!(parse_duration_to_seconds("").is_err());
        assert!(parse_duration_to_seconds("0m").is_err());
        assert!(parse_duration_to_seconds("-5m").is_err());
        assert!(parse_duration_to_seconds("5x").is_err());
        assert!(parse_duration_to_seconds("abc").is_err());
    }

    #[test]
    fn first_scalar_extracts_vector_value() {
        let body = serde_json::json!({
            "status": "success",
            "data": {
                "resultType": "vector",
                "result": [
                    { "metric": {}, "value": [1714574400, "0.99973"] }
                ]
            }
        });
        let v = first_scalar(&body).expect("has value");
        assert!((v - 0.99973).abs() < 1e-9);
    }

    #[test]
    fn first_scalar_extracts_scalar_value() {
        let body = serde_json::json!({
            "status": "success",
            "data": {
                "resultType": "scalar",
                "result": [1714574400, "42.5"]
            }
        });
        assert_eq!(first_scalar(&body).unwrap(), 42.5);
    }

    #[test]
    fn first_scalar_returns_none_on_empty_or_bad_shapes() {
        let empty = serde_json::json!({
            "status": "success",
            "data": { "resultType": "vector", "result": [] }
        });
        assert!(first_scalar(&empty).is_none());

        let missing = serde_json::json!({"status": "success"});
        assert!(first_scalar(&missing).is_none());

        let matrix = serde_json::json!({
            "data": { "resultType": "matrix", "result": [] }
        });
        assert!(first_scalar(&matrix).is_none());
    }
}
