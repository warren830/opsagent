use axum::{Json, extract::State};
use serde::Deserialize;
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

use crate::AppState;
use crate::error::AppResult;
use crate::services;

/// Look up the `id` of the open issue we just upserted so we can link it to
/// an `slo_burn_events` row. Matches by `(source, fingerprint)` — the same
/// tuple `upsert_issue` uses for its own dedup check — and returns `None`
/// if no open row exists (e.g. a resolving alert or a failed insert).
///
/// Kept here rather than in `services::alerts` because the SLO ingestion is
/// the only caller today; can be hoisted if another handler needs it.
async fn find_open_issue_id(pool: &PgPool, source: &str, fingerprint: &str) -> Option<Uuid> {
    sqlx::query_scalar::<_, Uuid>(
        r#"SELECT id FROM issues
           WHERE source = $1 AND status != 'resolved'
             AND rca_result @> $2::jsonb
           ORDER BY created_at DESC
           LIMIT 1"#,
    )
    .bind(source)
    .bind(serde_json::json!({"fingerprint": fingerprint}).to_string())
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
}

// ─── Grafana ──────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrafanaWebhook {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    alerts: Vec<GrafanaAlert>,
    #[serde(default)]
    #[allow(dead_code)]
    common_labels: Option<serde_json::Value>,
    #[serde(default)]
    #[allow(dead_code)]
    common_annotations: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrafanaAlert {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    labels: Option<serde_json::Value>,
    #[serde(default)]
    annotations: Option<serde_json::Value>,
    #[serde(default)]
    starts_at: Option<String>,
    #[serde(default)]
    fingerprint: Option<String>,
}

/// POST /api/alerts — receive Grafana alerting webhooks.
pub async fn receive(
    State(state): State<AppState>,
    Json(payload): Json<GrafanaWebhook>,
) -> AppResult<Json<serde_json::Value>> {
    let alert_count = payload.alerts.len();
    tracing::info!(
        "Received Grafana webhook: status={:?}, {} alert(s)",
        payload.status,
        alert_count
    );

    let rca_ctx = services::alerts::RcaContext {
        pool: state.pool.clone(),
        registry: state.rca_registry.clone(),
        config: Arc::new(state.config.clone()),
    };

    let mut created = 0u64;
    let mut resolved = 0u64;

    for alert in &payload.alerts {
        let alert_status = alert.status.as_deref().unwrap_or("firing");
        let labels = alert.labels.as_ref();
        let annotations = alert.annotations.as_ref();

        let alertname = labels
            .and_then(|l| l.get("alertname"))
            .and_then(|v| v.as_str())
            .unwrap_or("Unnamed Alert");

        let summary = annotations.and_then(|a| a.get("summary")).and_then(|v| v.as_str());

        let description = annotations.and_then(|a| a.get("description")).and_then(|v| v.as_str());

        let severity_raw = labels
            .and_then(|l| l.get("severity"))
            .and_then(|v| v.as_str())
            .unwrap_or("medium");

        let fingerprint = alert.fingerprint.as_deref().unwrap_or(alertname);

        let meta = serde_json::json!({
            "fingerprint": fingerprint,
            "starts_at": alert.starts_at,
            "labels": labels,
            "annotations": annotations,
        });

        let is_resolved = alert_status == "resolved";

        let (c, r) = services::alerts::upsert_issue(
            &state.pool,
            "grafana",
            fingerprint,
            summary.unwrap_or(alertname),
            description.unwrap_or(""),
            services::alerts::normalize_severity(severity_raw),
            &meta,
            is_resolved,
            "incident",
            Some(&rca_ctx),
        )
        .await;
        created += c;
        resolved += r;

        // If the alert carries SLO burn labels (see `rule_generator`), ingest
        // it into `slo_burn_events`. The issue link is best-effort: on
        // `resolved` the lookup returns `None` (the issue is already closed),
        // which is fine — resolve_open_burn matches by (slo_id, window).
        let issue_id = if is_resolved {
            None
        } else {
            find_open_issue_id(&state.pool, "grafana", fingerprint).await
        };
        if let Err(e) = services::slo::alert_ingestion::ingest_slo_burn_alert(
            &state.pool,
            labels,
            is_resolved,
            issue_id,
        )
        .await
        {
            tracing::warn!("SLO burn ingestion (grafana) failed: {}", e);
        }
    }

    Ok(Json(serde_json::json!({
        "status": "ok",
        "created": created,
        "resolved": resolved,
    })))
}

// ─── Datadog ──────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct DatadogWebhook {
    /// Unique monitor/alert ID — used for deduplication
    #[serde(default, alias = "alert_id")]
    pub id: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    /// "error" | "warning" | "info" | "success"
    #[serde(default)]
    pub alert_type: Option<String>,
    /// "normal" | "low"
    #[serde(default)]
    #[allow(dead_code)]
    pub priority: Option<String>,
    /// Comma-separated tags: "env:prod,service:web"
    #[serde(default)]
    pub tags: Option<String>,
    /// Unix timestamp
    #[serde(default)]
    #[allow(dead_code)]
    pub date: Option<i64>,
    /// Transition — Datadog sends "Triggered", "Recovered", etc.
    #[serde(default, alias = "alert_transition")]
    pub transition: Option<String>,
}

/// POST /api/alerts/datadog — receive Datadog webhook notifications.
pub async fn receive_datadog(
    State(state): State<AppState>,
    Json(payload): Json<DatadogWebhook>,
) -> AppResult<Json<serde_json::Value>> {
    tracing::info!(
        "Received Datadog webhook: title={:?}, type={:?}",
        payload.title,
        payload.alert_type
    );

    let rca_ctx = services::alerts::RcaContext {
        pool: state.pool.clone(),
        registry: state.rca_registry.clone(),
        config: Arc::new(state.config.clone()),
    };

    let alert_id = payload.id.as_deref().unwrap_or("unknown");
    let title = payload.title.as_deref().unwrap_or("Datadog Alert");
    let body = payload.body.as_deref().unwrap_or("");
    let alert_type = payload.alert_type.as_deref().unwrap_or("warning");

    let is_resolved =
        matches!(payload.transition.as_deref(), Some("Recovered") | Some("Resolved")) || alert_type == "success";

    let severity = services::alerts::normalize_severity(alert_type);

    let meta = serde_json::json!({
        "fingerprint": alert_id,
        "alert_type": alert_type,
        "tags": payload.tags,
        "transition": payload.transition,
    });

    let (created, resolved) = services::alerts::upsert_issue(
        &state.pool,
        "datadog",
        alert_id,
        title,
        body,
        severity,
        &meta,
        is_resolved,
        "incident",
        Some(&rca_ctx),
    )
    .await;

    // SLO burn ingestion — no-op for standard Datadog payloads (which don't
    // carry an `slo_id` label), but enabled for the case where Datadog is
    // configured to proxy Mimir-generated alerts that do.
    let issue_id = if is_resolved {
        None
    } else {
        find_open_issue_id(&state.pool, "datadog", alert_id).await
    };
    if let Err(e) = services::slo::alert_ingestion::ingest_slo_burn_alert(
        &state.pool,
        Some(&meta),
        is_resolved,
        issue_id,
    )
    .await
    {
        tracing::warn!("SLO burn ingestion (datadog) failed: {}", e);
    }

    Ok(Json(serde_json::json!({
        "status": "ok",
        "created": created,
        "resolved": resolved,
    })))
}

// ─── Dynatrace ────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct DynatraceWebhook {
    /// Unique problem ID, e.g. "P-12345"
    #[serde(default, alias = "ProblemID")]
    pub problem_id: Option<String>,
    /// "OPEN" | "RESOLVED" | "MERGED"
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default, alias = "ProblemTitle")]
    pub problem_title: Option<String>,
    /// "AVAILABILITY" | "ERROR" | "PERFORMANCE" | "RESOURCE_CONTENTION" | "CUSTOM_ALERT"
    #[serde(default, alias = "ProblemSeverity")]
    pub problem_severity: Option<String>,
    #[serde(default, alias = "ProblemURL")]
    pub problem_url: Option<String>,
    #[serde(default, alias = "ProblemImpact")]
    pub problem_impact: Option<String>,
    #[serde(default)]
    pub tags: Option<String>,
    /// Raw impacted entities (optional)
    #[serde(default, alias = "ImpactedEntities")]
    pub impacted_entities: Option<serde_json::Value>,
}

/// POST /api/alerts/dynatrace — receive Dynatrace problem notifications.
pub async fn receive_dynatrace(
    State(state): State<AppState>,
    Json(payload): Json<DynatraceWebhook>,
) -> AppResult<Json<serde_json::Value>> {
    tracing::info!(
        "Received Dynatrace webhook: problem={:?}, state={:?}",
        payload.problem_id,
        payload.state
    );

    let rca_ctx = services::alerts::RcaContext {
        pool: state.pool.clone(),
        registry: state.rca_registry.clone(),
        config: Arc::new(state.config.clone()),
    };

    let problem_id = payload.problem_id.as_deref().unwrap_or("unknown");
    let title = payload.problem_title.as_deref().unwrap_or("Dynatrace Problem");
    let severity_raw = payload.problem_severity.as_deref().unwrap_or("PERFORMANCE");
    let is_resolved = payload.state.as_deref() == Some("RESOLVED");

    let severity = services::alerts::normalize_severity(severity_raw);

    let meta = serde_json::json!({
        "fingerprint": problem_id,
        "state": payload.state,
        "problem_severity": severity_raw,
        "problem_url": payload.problem_url,
        "problem_impact": payload.problem_impact,
        "tags": payload.tags,
        "impacted_entities": payload.impacted_entities,
    });

    let description = format!(
        "Dynatrace Problem: {} ({})\n{}",
        title,
        severity_raw,
        payload.problem_url.as_deref().unwrap_or("")
    );

    let (created, resolved) = services::alerts::upsert_issue(
        &state.pool,
        "dynatrace",
        problem_id,
        title,
        &description,
        severity,
        &meta,
        is_resolved,
        "incident",
        Some(&rca_ctx),
    )
    .await;

    // SLO burn ingestion — same philosophy as the Datadog handler. No-op for
    // native Dynatrace payloads, active when Dynatrace is wired to proxy SLO
    // burn alerts that include the Mimir-style labels.
    let issue_id = if is_resolved {
        None
    } else {
        find_open_issue_id(&state.pool, "dynatrace", problem_id).await
    };
    if let Err(e) = services::slo::alert_ingestion::ingest_slo_burn_alert(
        &state.pool,
        Some(&meta),
        is_resolved,
        issue_id,
    )
    .await
    {
        tracing::warn!("SLO burn ingestion (dynatrace) failed: {}", e);
    }

    Ok(Json(serde_json::json!({
        "status": "ok",
        "created": created,
        "resolved": resolved,
    })))
}
