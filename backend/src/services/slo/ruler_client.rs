//! Mimir ruler client for pushing and deleting SLO rule groups.
//!
//! Mimir exposes a Prometheus-compatible ruler API:
//!
//! * `POST   /api/v1/rules/{namespace}` — upsert a rule group (body = YAML).
//! * `GET    /api/v1/rules/{namespace}/{group}` — fetch a single group.
//! * `DELETE /api/v1/rules/{namespace}/{group}` — delete a group.
//!
//! Tenancy is conveyed via the `X-Scope-OrgID` header when the backend is
//! multi-tenant Mimir. Basic-auth credentials (cloud Grafana) mirror what
//! [`super::mimir_client`] already derives from the `telemetry_config` row.
//!
//! The client intentionally stays small — no retries, no rate limiting. The
//! call sites swallow push failures as warnings (W3 design decision: SLO DB
//! record is authoritative; ruler drift is recoverable via the manual
//! `/sync-rules` endpoint).

use crate::error::{AppError, AppResult};
use crate::models::telemetry::TelemetryConfig;
use reqwest::{Client, StatusCode};
use std::sync::OnceLock;
use std::time::Duration;

/// Process-wide `reqwest::Client` for the ruler API. Built once with the
/// same timeout/pool defaults as the main HTTP client in `main.rs` so
/// ruler HTTP calls can't stall forever and can't leak pools. P1 #6.
static RULER_CLIENT: OnceLock<Client> = OnceLock::new();

fn ruler_http_client() -> Client {
    RULER_CLIENT
        .get_or_init(|| {
            Client::builder()
                .timeout(Duration::from_secs(15))
                .connect_timeout(Duration::from_secs(5))
                .pool_max_idle_per_host(10)
                .build()
                .unwrap_or_else(|_| Client::new())
        })
        .clone()
}

/// Thin wrapper around a Mimir ruler HTTP endpoint.
#[derive(Debug, Clone)]
pub struct RulerClient {
    http: Client,
    base_url: String,
    basic_auth: Option<(String, String)>,
    tenant_header: Option<String>,
}

impl RulerClient {
    /// Build a client from a `telemetry_config` row. Accepts the same config
    /// shape the Mimir query proxy uses (`mimir_endpoint_url` + optional
    /// `mimir_user_id` / `api_token` / `tenant_id`).
    ///
    /// Returns `AppError::BadRequest` when the provider isn't `mimir`-like or
    /// the endpoint URL is missing, so the caller can report a clean error.
    pub fn from_telemetry_config(config: &TelemetryConfig) -> AppResult<Self> {
        // Accept any provider that routes metrics — the URL shape is what we
        // actually care about, not the provider label. This keeps us tolerant
        // of `grafana_cloud`, `mimir`, `self-hosted` all pointing at the same
        // HTTP API.
        let url = config
            .config
            .get("mimir_endpoint_url")
            .or_else(|| config.config.get("mimir_endpoint"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                AppError::BadRequest(
                    "Telemetry config is missing `mimir_endpoint_url`.".to_string(),
                )
            })?
            .trim_end_matches('/')
            .to_string();

        let basic_auth = if config.provider != "self-hosted" {
            let user = config
                .config
                .get("mimir_user_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let token = config
                .config
                .get("api_token")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if !user.is_empty() && !token.is_empty() {
                Some((user.to_string(), token.to_string()))
            } else {
                None
            }
        } else {
            None
        };

        // `tenant_id` in the config blob (when present) maps onto the Mimir
        // X-Scope-OrgID header. Distinct from the row's `tenant_id` which is
        // our own platform tenancy.
        let tenant_header = config
            .config
            .get("mimir_tenant_id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_string);

        Ok(Self {
            http: ruler_http_client(),
            base_url: url,
            basic_auth,
            tenant_header,
        })
    }

    /// POST a rule group YAML to `{base}/api/v1/rules/{namespace}`.
    ///
    /// Mimir expects `Content-Type: application/yaml`. The group name is
    /// parsed from the YAML itself by the ruler — we just send the full
    /// group document.
    pub async fn upsert_rules(&self, namespace: &str, group_yaml: &str) -> AppResult<()> {
        let url = format!("{}/api/v1/rules/{}", self.base_url, namespace);
        let mut req = self
            .http
            .post(&url)
            .header("Content-Type", "application/yaml")
            .body(group_yaml.to_string());
        req = self.apply_auth(req);

        let response = req.send().await.map_err(|e| {
            AppError::HttpClient(format!("Mimir ruler upsert_rules failed: {}", e))
        })?;

        self.expect_success(response, "upsert_rules").await
    }

    /// DELETE a rule group. 404 is treated as success (idempotent).
    pub async fn delete_rules(&self, namespace: &str, group_name: &str) -> AppResult<()> {
        let url = format!("{}/api/v1/rules/{}/{}", self.base_url, namespace, group_name);
        let mut req = self.http.delete(&url);
        req = self.apply_auth(req);

        let response = req.send().await.map_err(|e| {
            AppError::HttpClient(format!("Mimir ruler delete_rules failed: {}", e))
        })?;

        if response.status() == StatusCode::NOT_FOUND {
            return Ok(());
        }
        self.expect_success(response, "delete_rules").await
    }

    /// GET the currently installed rule group YAML for drift detection.
    /// Returns `None` when the ruler reports 404.
    pub async fn get_rules(
        &self,
        namespace: &str,
        group_name: &str,
    ) -> AppResult<Option<String>> {
        let url = format!("{}/api/v1/rules/{}/{}", self.base_url, namespace, group_name);
        let mut req = self.http.get(&url);
        req = self.apply_auth(req);

        let response = req.send().await.map_err(|e| {
            AppError::HttpClient(format!("Mimir ruler get_rules failed: {}", e))
        })?;

        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(status_to_error(status, "get_rules", &body));
        }
        let body = response
            .text()
            .await
            .map_err(|e| AppError::HttpClient(format!("Mimir ruler get_rules body: {}", e)))?;
        Ok(Some(body))
    }

    fn apply_auth(&self, mut req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if let Some((user, token)) = &self.basic_auth {
            req = req.basic_auth(user, Some(token));
        }
        if let Some(tenant) = &self.tenant_header {
            req = req.header("X-Scope-OrgID", tenant);
        }
        req
    }

    async fn expect_success(&self, response: reqwest::Response, op: &str) -> AppResult<()> {
        let status = response.status();
        if status.is_success() {
            return Ok(());
        }
        let body = response.text().await.unwrap_or_default();
        Err(status_to_error(status, op, &body))
    }
}

fn status_to_error(status: StatusCode, op: &str, body: &str) -> AppError {
    let snippet = body.chars().take(500).collect::<String>();
    if status.is_client_error() {
        AppError::BadRequest(format!("Mimir ruler {} rejected ({}): {}", op, status, snippet))
    } else {
        AppError::HttpClient(format!(
            "Mimir ruler {} failed ({}): {}",
            op, status, snippet
        ))
    }
}

/// Resolve the first telemetry config with `routing.signals` including
/// `"metrics"` for a given tenant scope. Mirrors the discovery logic of
/// [`super::mimir_client::resolve_metrics_endpoint`] but returns the full
/// row so [`RulerClient::from_telemetry_config`] can read auth fields.
///
/// **Strict tenant scoping**: when `tenant_id` is `Some(_)` we only match
/// rows with that exact `tenant_id`. A missing tenant config does *not*
/// fall back to the global (`tenant_id IS NULL`) row — that fallback
/// could route a tenant's SLO recording rules into another tenant's
/// Mimir namespace, exposing their rules and alert evaluations. Only a
/// `None` caller (genuinely tenant-less ops) reads the global row.
pub async fn resolve_ruler_config(
    pool: &sqlx::PgPool,
    tenant_id: Option<uuid::Uuid>,
) -> AppResult<Option<TelemetryConfig>> {
    if let Some(tid) = tenant_id {
        let row = sqlx::query_as::<_, TelemetryConfig>(
            r#"SELECT * FROM telemetry_config
               WHERE enabled = true
                 AND tenant_id = $1
                 AND routing->'signals' ? 'metrics'
               ORDER BY created_at DESC LIMIT 1"#,
        )
        .bind(tid)
        .fetch_optional(pool)
        .await?;
        // No cross-tenant fallback — return whatever the tenant-scoped
        // query produced (possibly None).
        return Ok(row);
    }

    // Only reached when the caller has no tenant at all.
    let global = sqlx::query_as::<_, TelemetryConfig>(
        r#"SELECT * FROM telemetry_config
           WHERE enabled = true
             AND tenant_id IS NULL
             AND routing->'signals' ? 'metrics'
           ORDER BY created_at DESC LIMIT 1"#,
    )
    .fetch_optional(pool)
    .await?;
    Ok(global)
}

// ---------------------------------------------------------------------------
// Tests — no network I/O. Actual HTTP behaviour is covered by integration
// tests in W7 which spin up wiremock.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use serde_json::json;
    use uuid::Uuid;

    fn cfg(provider: &str, config: serde_json::Value) -> TelemetryConfig {
        TelemetryConfig {
            id: Uuid::new_v4(),
            name: "test".to_string(),
            provider: provider.to_string(),
            config,
            routing: json!({"signals": ["metrics"], "scope": "all"}),
            enabled: true,
            tenant_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn from_config_resolves_self_hosted_without_auth() {
        let c = cfg(
            "self-hosted",
            json!({"mimir_endpoint_url": "http://mimir.internal:9009/"}),
        );
        let client = RulerClient::from_telemetry_config(&c).unwrap();
        assert_eq!(client.base_url, "http://mimir.internal:9009");
        assert!(client.basic_auth.is_none());
        assert!(client.tenant_header.is_none());
    }

    #[test]
    fn from_config_resolves_grafana_cloud_with_basic_auth() {
        let c = cfg(
            "grafana_cloud",
            json!({
                "mimir_endpoint_url": "https://prometheus.grafana.net/api/prom",
                "mimir_user_id": "12345",
                "api_token": "secret-token",
                "mimir_tenant_id": "tenant-a"
            }),
        );
        let client = RulerClient::from_telemetry_config(&c).unwrap();
        assert_eq!(client.base_url, "https://prometheus.grafana.net/api/prom");
        assert_eq!(
            client.basic_auth,
            Some(("12345".to_string(), "secret-token".to_string()))
        );
        assert_eq!(client.tenant_header, Some("tenant-a".to_string()));
    }

    #[test]
    fn from_config_rejects_missing_endpoint() {
        let c = cfg("mimir", json!({}));
        let err = RulerClient::from_telemetry_config(&c).unwrap_err();
        match err {
            AppError::BadRequest(msg) => assert!(msg.contains("mimir_endpoint_url")),
            _ => panic!("expected BadRequest"),
        }
    }

    #[test]
    fn status_to_error_maps_4xx_to_bad_request() {
        let err = status_to_error(StatusCode::BAD_REQUEST, "upsert_rules", "bad yaml");
        match err {
            AppError::BadRequest(msg) => {
                assert!(msg.contains("upsert_rules"));
                assert!(msg.contains("bad yaml"));
            }
            _ => panic!("expected BadRequest"),
        }
    }

    #[test]
    fn status_to_error_maps_5xx_to_http_client() {
        let err = status_to_error(StatusCode::INTERNAL_SERVER_ERROR, "delete_rules", "down");
        match err {
            AppError::HttpClient(msg) => {
                assert!(msg.contains("delete_rules"));
                assert!(msg.contains("down"));
            }
            _ => panic!("expected HttpClient"),
        }
    }
}
