mod helpers;

use sqlx::PgPool;
use openops::services::alerts;

#[sqlx::test(migrations = "src/migrations")]
async fn test_upsert_creates_issue(pool: PgPool) {
    let meta = serde_json::json!({"fingerprint": "fp-001"});
    let (created, resolved) = alerts::upsert_issue(
        &pool, "grafana", "fp-001", "Test Alert", "desc", "high", &meta, false, "incident", None,
    ).await;
    assert_eq!(created, 1);
    assert_eq!(resolved, 0);
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_upsert_deduplicates(pool: PgPool) {
    let meta = serde_json::json!({"fingerprint": "fp-dup"});
    alerts::upsert_issue(&pool, "grafana", "fp-dup", "Alert", "", "high", &meta, false, "incident", None).await;
    let (created, _) = alerts::upsert_issue(&pool, "grafana", "fp-dup", "Alert", "", "high", &meta, false, "incident", None).await;
    assert_eq!(created, 0); // deduplicated
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_upsert_resolves(pool: PgPool) {
    let meta = serde_json::json!({"fingerprint": "fp-res"});
    alerts::upsert_issue(&pool, "datadog", "fp-res", "Alert", "", "high", &meta, false, "incident", None).await;
    let (_, resolved) = alerts::upsert_issue(&pool, "datadog", "fp-res", "Alert", "", "high", &meta, true, "incident", None).await;
    assert_eq!(resolved, 1);
}

#[test]
fn test_normalize_severity() {
    assert_eq!(alerts::normalize_severity("critical"), "critical");
    assert_eq!(alerts::normalize_severity("P1"), "critical");
    assert_eq!(alerts::normalize_severity("warning"), "high");
    assert_eq!(alerts::normalize_severity("error"), "high");
    assert_eq!(alerts::normalize_severity("info"), "low");
    assert_eq!(alerts::normalize_severity("unknown_value"), "medium");
    assert_eq!(alerts::normalize_severity("AVAILABILITY"), "critical");
}
