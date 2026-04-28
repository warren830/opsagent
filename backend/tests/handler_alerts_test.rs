//! HTTP-level tests for /api/alerts* — public webhook ingress.
//! No auth middleware — external services can't send JWT.

mod helpers;

use axum::{Router, http::StatusCode, routing::post};
use helpers::http::{body_json, call, json_request, test_state};
use ops::handlers;
use serde_json::json;
use sqlx::PgPool;

fn app(pool: PgPool) -> Router {
    Router::new()
        .route("/api/alerts", post(handlers::alerts::receive))
        .route("/api/alerts/datadog", post(handlers::alerts::receive_datadog))
        .route("/api/alerts/dynatrace", post(handlers::alerts::receive_dynatrace))
        .with_state(test_state(pool))
}

// ---- Grafana ----

#[sqlx::test(migrations = "src/migrations")]
async fn grafana_firing_alert_creates_issue(pool: PgPool) {
    let resp = call(
        app(pool.clone()),
        json_request(
            "POST",
            "/api/alerts",
            None,
            &json!({
                "status": "firing",
                "alerts": [{
                    "status": "firing",
                    "labels": {"alertname": "HighCPU", "severity": "critical"},
                    "annotations": {"summary": "CPU too high", "description": "prod-1 at 95%"},
                    "fingerprint": "fp-001",
                }],
            }),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(body_json(resp).await["created"], 1);

    // Confirm issue landed in DB
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM issues WHERE source = 'grafana'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count.0, 1);
}

#[sqlx::test(migrations = "src/migrations")]
async fn grafana_same_fingerprint_dedups(pool: PgPool) {
    let payload = json!({
        "alerts": [{
            "status": "firing",
            "labels": {"alertname": "HighCPU", "severity": "critical"},
            "fingerprint": "fp-dedup",
        }],
    });

    call(app(pool.clone()), json_request("POST", "/api/alerts", None, &payload)).await;
    call(app(pool.clone()), json_request("POST", "/api/alerts", None, &payload)).await;

    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM issues WHERE source = 'grafana'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count.0, 1, "same fingerprint must dedup to one issue");
}

#[sqlx::test(migrations = "src/migrations")]
async fn grafana_resolved_status_marks_issue_resolved(pool: PgPool) {
    // First firing
    call(
        app(pool.clone()),
        json_request(
            "POST",
            "/api/alerts",
            None,
            &json!({
                "alerts": [{
                    "status": "firing",
                    "labels": {"alertname": "A", "severity": "high"},
                    "fingerprint": "fp-resolve",
                }],
            }),
        ),
    )
    .await;
    // Then resolved
    let resp = call(
        app(pool.clone()),
        json_request(
            "POST",
            "/api/alerts",
            None,
            &json!({
                "alerts": [{
                    "status": "resolved",
                    "labels": {"alertname": "A", "severity": "high"},
                    "fingerprint": "fp-resolve",
                }],
            }),
        ),
    )
    .await;
    assert_eq!(body_json(resp).await["resolved"], 1);
}

// ---- Datadog ----

#[sqlx::test(migrations = "src/migrations")]
async fn datadog_triggered_creates_issue(pool: PgPool) {
    let resp = call(
        app(pool.clone()),
        json_request(
            "POST",
            "/api/alerts/datadog",
            None,
            &json!({
                "id": "dd-123",
                "title": "API error rate > 5%",
                "body": "Recurring 500s",
                "alert_type": "error",
                "transition": "Triggered",
            }),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(body_json(resp).await["created"], 1);
}

#[sqlx::test(migrations = "src/migrations")]
async fn datadog_recovered_resolves_issue(pool: PgPool) {
    let payload_open = json!({
        "id": "dd-recov",
        "title": "t",
        "alert_type": "error",
        "transition": "Triggered",
    });
    let payload_close = json!({
        "id": "dd-recov",
        "title": "t",
        "alert_type": "success",
        "transition": "Recovered",
    });
    call(app(pool.clone()), json_request("POST", "/api/alerts/datadog", None, &payload_open)).await;
    let resp = call(
        app(pool.clone()),
        json_request("POST", "/api/alerts/datadog", None, &payload_close),
    )
    .await;
    assert_eq!(body_json(resp).await["resolved"], 1);
}

// ---- Dynatrace ----

#[sqlx::test(migrations = "src/migrations")]
async fn dynatrace_open_creates_issue(pool: PgPool) {
    let resp = call(
        app(pool.clone()),
        json_request(
            "POST",
            "/api/alerts/dynatrace",
            None,
            &json!({
                "ProblemID": "P-42",
                "ProblemTitle": "Slow response",
                "ProblemSeverity": "PERFORMANCE",
                "State": "OPEN",
            }),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(body_json(resp).await["created"], 1);
}

#[sqlx::test(migrations = "src/migrations")]
async fn dynatrace_resolved_marks_resolved(pool: PgPool) {
    let open = json!({"ProblemID": "P-99", "ProblemTitle": "t", "ProblemSeverity": "ERROR", "State": "OPEN"});
    let close = json!({"ProblemID": "P-99", "ProblemTitle": "t", "ProblemSeverity": "ERROR", "State": "RESOLVED"});
    call(app(pool.clone()), json_request("POST", "/api/alerts/dynatrace", None, &open)).await;
    let resp = call(
        app(pool.clone()),
        json_request("POST", "/api/alerts/dynatrace", None, &close),
    )
    .await;
    assert_eq!(body_json(resp).await["resolved"], 1);
}

// ---- Error shape ----

#[sqlx::test(migrations = "src/migrations")]
async fn malformed_grafana_body_returns_400(pool: PgPool) {
    // Send raw non-JSON-parseable body
    let req = axum::http::Request::builder()
        .method("POST")
        .uri("/api/alerts")
        .header("Content-Type", "application/json")
        .body(axum::body::Body::from("not-json"))
        .unwrap();
    let resp = call(app(pool), req).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}
