use axum::{Json, extract::State};
use serde::Deserialize;

use crate::AppState;
use crate::error::AppResult;

// ─── Shared helpers ───────────────────────────────────────────

/// Deduplicate + create/resolve an issue from any alert source.
/// `source`: "grafana" | "datadog" | "dynatrace"
/// `dedup_key`: unique identifier for deduplication (fingerprint / alert_id / problem_id)
#[allow(clippy::too_many_arguments)]
pub async fn upsert_issue(
    pool: &sqlx::PgPool,
    source: &str,
    dedup_key: &str,
    title: &str,
    description: &str,
    severity: &str,
    meta: &serde_json::Value,
    is_resolved: bool,
    issue_type: &str,
) -> (u64, u64) {
    let mut created = 0u64;
    let mut resolved = 0u64;

    if is_resolved {
        let result = sqlx::query(
            r#"UPDATE issues SET status = 'resolved', resolved_at = NOW(), updated_at = NOW()
               WHERE source = $1 AND status != 'resolved'
               AND rca_result @> $2::jsonb"#,
        )
        .bind(source)
        .bind(serde_json::json!({"fingerprint": dedup_key}).to_string())
        .execute(pool)
        .await;
        if let Ok(r) = result {
            resolved = r.rows_affected();
        }
        return (created, resolved);
    }

    // Skip duplicate
    let existing = sqlx::query_scalar::<_, i64>(
        r#"SELECT COUNT(*) FROM issues
           WHERE source = $1 AND status != 'resolved'
           AND rca_result @> $2::jsonb"#,
    )
    .bind(source)
    .bind(serde_json::json!({"fingerprint": dedup_key}).to_string())
    .fetch_one(pool)
    .await
    .unwrap_or(0);

    if existing > 0 {
        tracing::debug!("Skipping duplicate {} alert: key={}", source, dedup_key);
        return (created, resolved);
    }

    let result = sqlx::query(
        r#"INSERT INTO issues (title, description, source, severity, status, rca_result, issue_type)
           VALUES ($1, $2, $3, $4, 'open', $5, $6)"#,
    )
    .bind(title)
    .bind(description)
    .bind(source)
    .bind(severity)
    .bind(meta)
    .bind(issue_type)
    .execute(pool)
    .await;

    match result {
        Ok(_) => {
            created = 1;
            tracing::info!(
                "Created issue from {} alert: title={}, severity={}",
                source,
                title,
                severity
            );
        }
        Err(e) => {
            tracing::error!("Failed to create issue from {} alert: {}", source, e);
        }
    }

    (created, resolved)
}

/// Normalize severity string from various providers to our enum.
pub fn normalize_severity(raw: &str) -> &'static str {
    match raw.to_lowercase().as_str() {
        "critical" | "p1" | "availability" => "critical",
        "high" | "warning" | "p2" | "error" | "resource_contention" => "high",
        "low" | "info" | "p4" | "p5" | "custom_alert" => "low",
        _ => "medium",
    }
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

        let (c, r) = upsert_issue(
            &state.pool,
            "grafana",
            fingerprint,
            summary.unwrap_or(alertname),
            description.unwrap_or(""),
            normalize_severity(severity_raw),
            &meta,
            alert_status == "resolved",
            "incident",
        )
        .await;
        created += c;
        resolved += r;
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

    let alert_id = payload.id.as_deref().unwrap_or("unknown");
    let title = payload.title.as_deref().unwrap_or("Datadog Alert");
    let body = payload.body.as_deref().unwrap_or("");
    let alert_type = payload.alert_type.as_deref().unwrap_or("warning");

    let is_resolved =
        matches!(payload.transition.as_deref(), Some("Recovered") | Some("Resolved")) || alert_type == "success";

    let severity = normalize_severity(alert_type);

    let meta = serde_json::json!({
        "fingerprint": alert_id,
        "alert_type": alert_type,
        "tags": payload.tags,
        "transition": payload.transition,
    });

    let (created, resolved) = upsert_issue(
        &state.pool,
        "datadog",
        alert_id,
        title,
        body,
        severity,
        &meta,
        is_resolved,
        "incident",
    )
    .await;

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

    let problem_id = payload.problem_id.as_deref().unwrap_or("unknown");
    let title = payload.problem_title.as_deref().unwrap_or("Dynatrace Problem");
    let severity_raw = payload.problem_severity.as_deref().unwrap_or("PERFORMANCE");
    let is_resolved = payload.state.as_deref() == Some("RESOLVED");

    let severity = normalize_severity(severity_raw);

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

    let (created, resolved) = upsert_issue(
        &state.pool,
        "dynatrace",
        problem_id,
        title,
        &description,
        severity,
        &meta,
        is_resolved,
        "incident",
    )
    .await;

    Ok(Json(serde_json::json!({
        "status": "ok",
        "created": created,
        "resolved": resolved,
    })))
}
