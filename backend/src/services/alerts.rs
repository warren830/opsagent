use std::sync::Arc;
use sqlx::PgPool;

use crate::config::AppConfig;
use crate::services::rca::RcaRegistry;

/// Context needed for auto-triggering RCA on critical/high alerts.
/// Avoids depending on AppState in the service layer.
pub struct RcaContext {
    pub pool: PgPool,
    pub registry: Arc<RcaRegistry>,
    pub config: Arc<AppConfig>,
}

/// Deduplicate + create/resolve an issue from any alert source.
/// Returns (created_count, resolved_count).
#[allow(clippy::too_many_arguments)]
pub async fn upsert_issue(
    pool: &PgPool,
    source: &str,
    dedup_key: &str,
    title: &str,
    description: &str,
    severity: &str,
    meta: &serde_json::Value,
    is_resolved: bool,
    issue_type: &str,
    rca_context: Option<&RcaContext>,
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

            // Auto-pause progressing rollouts on critical/high alerts
            if severity == "critical" || severity == "high" {
                if let Some(namespace) = crate::services::rollout_guard::extract_namespace_from_alert(meta) {
                    let pool_clone = pool.clone();
                    let ns = namespace.clone();
                    tokio::spawn(async move {
                        let paused = crate::services::rollout_guard::check_and_pause_rollouts(&pool_clone, &ns).await;
                        if !paused.is_empty() {
                            tracing::warn!("Alert guard auto-paused rollouts: {:?}", paused);
                        }
                    });
                }

                // Auto-trigger RCA on critical/high alerts
                if let Some(ctx) = rca_context {
                    let pool_clone = ctx.pool.clone();
                    let registry = ctx.registry.clone();
                    let config = ctx.config.clone();
                    let title_str = title.to_string();
                    tokio::spawn(async move {
                        // Fetch the just-created issue
                        let issue = sqlx::query_as::<_, crate::models::issue::Issue>(
                            "SELECT * FROM issues WHERE title = $1 AND status = 'open' ORDER BY created_at DESC LIMIT 1",
                        )
                        .bind(&title_str)
                        .fetch_optional(&pool_clone)
                        .await;

                        if let Ok(Some(issue)) = issue {
                            tracing::info!("Auto-triggering RCA for critical/high issue: {}", issue.id);
                            crate::services::rca::run_rca(pool_clone, config, registry, issue).await;
                        }
                    });
                }
            }
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
