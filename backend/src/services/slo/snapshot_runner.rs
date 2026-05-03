//! Periodic snapshot scheduler for the SLO engine (W5 batch-1).
//!
//! Every `SLO_SNAPSHOT_INTERVAL_SECS` (default 300 s), iterate over every
//! enabled SLO, query Mimir for the current SLI achievement + burn rates,
//! and persist one row in `error_budget_snapshots`.
//!
//! Design notes:
//!
//! * **One call path at 5 min cadence** — keeps the DB write volume bounded
//!   (~288 rows per SLO per day).
//! * **Loop isolation** — a per-SLO Mimir failure is logged and the loop
//!   continues; we never panic out of the tick.
//! * **Recording-rule fast path** — when `recording_rules_hash` is set on
//!   the SLO row (i.e. W3 ruler sync succeeded), we query the pre-aggregated
//!   metric `sli:slo_<short>:ratio_rate<window>`. Otherwise we fall back to
//!   the raw `good / total` division so even an un-synced SLO gets snapshots.
//! * **Burn rate windows** — we capture 1 h / 6 h / 24 h / 3 d rates as
//!   columns on the snapshot. 24 h is derived on the fly from `ratio_rate3d`
//!   via a nested `avg_over_time` when the recording rule isn't present
//!   (the ruleset doesn't emit a 24 h window today).

use crate::error::AppResult;
use crate::models::slo::Slo;
use crate::services::slo::{budget_calc, mimir_client, rule_generator};
use chrono::{Duration as ChronoDuration, Utc};
use futures::stream::{self, StreamExt};
use sqlx::PgPool;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Cap on concurrent per-SLO snapshot work. Each `snapshot_one` issues up to
/// five Mimir `/api/v1/query` requests plus one DB insert, so 8-way
/// parallelism is comfortable for the 15s client timeout without burying the
/// ruler. P1 #7.
const SNAPSHOT_CONCURRENCY: usize = 8;

/// Default loop interval — 5 minutes aligns with `idx_budget_snapshots_slo_time`
/// being denormalised enough to support 90-day budget history charts.
const DEFAULT_INTERVAL_SECS: u64 = 300;

/// Entry point spawned by `main.rs`. Blocks forever; errors inside the loop
/// are logged and never bubble out so the background task cannot crash the
/// process.
pub async fn run_snapshot_loop(pool: PgPool) {
    let interval_secs: u64 = std::env::var("SLO_SNAPSHOT_INTERVAL_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_INTERVAL_SECS);
    tracing::info!("SLO snapshot scheduler started (interval={}s)", interval_secs);

    let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));
    // Skip the first immediate tick — let the server warm up, just like the
    // prediction / cluster-discovery schedulers in main.rs.
    interval.tick().await;

    loop {
        interval.tick().await;
        match snapshot_all_enabled(&pool).await {
            Ok(r) => {
                tracing::info!(
                    "SLO snapshot cycle: total={} succeeded={} failed={}",
                    r.total,
                    r.succeeded,
                    r.failed
                );
            }
            Err(e) => {
                tracing::error!("SLO snapshot loop error: {}", e);
            }
        }
    }
}

/// Result of one scheduler pass — exposed so tests / manual runs can assert.
#[derive(Debug, Default, Clone, Copy)]
pub struct SnapshotCycleResult {
    pub total: usize,
    pub succeeded: usize,
    pub failed: usize,
}

/// Single-pass snapshot — fetches all enabled SLOs and writes one snapshot
/// each. Returns a summary for logging; per-SLO errors are swallowed (logged
/// via `tracing::warn`) so one broken SLO doesn't stall the cycle.
pub async fn snapshot_all_enabled(pool: &PgPool) -> AppResult<SnapshotCycleResult> {
    let slos = sqlx::query_as::<_, Slo>(
        r#"SELECT * FROM slos
           WHERE enabled = TRUE
           ORDER BY created_at ASC"#,
    )
    .fetch_all(pool)
    .await?;

    let mut result = SnapshotCycleResult {
        total: slos.len(),
        ..Default::default()
    };
    if slos.is_empty() {
        return Ok(result);
    }

    // Resolve the Mimir endpoint once per cycle — same backend serves every
    // SLO. If the endpoint isn't configured we short-circuit so no per-SLO
    // log-spam surfaces.
    let endpoint = match mimir_client::resolve_metrics_endpoint(pool).await {
        Ok(ep) => ep,
        Err(e) => {
            tracing::debug!(
                "SLO snapshot skipped: no Mimir backend configured ({})",
                e
            );
            return Ok(result);
        }
    };

    // Run snapshots concurrently, bounded by `SNAPSHOT_CONCURRENCY`, so one
    // slow Mimir response can't serialise the entire cycle. Each task owns
    // its own per-SLO error handling; the atomics aggregate counts across
    // the stream.
    let succeeded = Arc::new(AtomicUsize::new(0));
    let failed = Arc::new(AtomicUsize::new(0));

    stream::iter(slos.into_iter())
        .for_each_concurrent(Some(SNAPSHOT_CONCURRENCY), |slo| {
            let pool = pool.clone();
            let endpoint = endpoint.clone();
            let succeeded = succeeded.clone();
            let failed = failed.clone();
            async move {
                match snapshot_one(&pool, &endpoint, &slo).await {
                    Ok(()) => {
                        succeeded.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(e) => {
                        failed.fetch_add(1, Ordering::Relaxed);
                        tracing::warn!(
                            slo_id = %slo.id,
                            name = %slo.name,
                            error = %e,
                            "SLO snapshot failed for this SLO"
                        );
                    }
                }
            }
        })
        .await;

    result.succeeded = succeeded.load(Ordering::Relaxed);
    result.failed = failed.load(Ordering::Relaxed);
    Ok(result)
}

/// Query Mimir for a single SLO and insert one snapshot row.
async fn snapshot_one(
    pool: &PgPool,
    endpoint: &mimir_client::MetricsEndpoint,
    slo: &Slo,
) -> AppResult<()> {
    let now_ts = Utc::now().timestamp();
    let window_query = sli_ratio_query(slo, slo.window_days);

    let sli_ratio = mimir_client::query_instant(endpoint, &window_query, Some(now_ts))
        .await
        .ok()
        .as_ref()
        .and_then(mimir_client::first_scalar)
        // Clamp to sane range: Mimir can emit NaN or >1 on boundary conditions.
        .map(|v| v.clamp(0.0, 1.0));

    // If Mimir returned nothing yet (new SLO, empty series), record a
    // placeholder snapshot with sli=1.0 so charts have a baseline rather
    // than a gap. Downstream UI can distinguish "no budget consumed" from
    // "no data" by looking at contiguity.
    let sli_ratio = sli_ratio.unwrap_or(1.0);
    let sli_achieved_pct = sli_ratio * 100.0;
    let budget_total = budget_calc::total_minutes(slo.objective_pct, slo.window_days);
    let budget_consumed = budget_calc::consumed_minutes(sli_achieved_pct, slo.window_days);
    let budget_remaining_pct = budget_calc::remaining_pct(budget_total, budget_consumed);

    let burn_1h = fetch_burn_rate(endpoint, slo, "1h", now_ts).await;
    let burn_6h = fetch_burn_rate(endpoint, slo, "6h", now_ts).await;
    let burn_24h = fetch_burn_rate(endpoint, slo, "24h", now_ts).await;
    let burn_3d = fetch_burn_rate(endpoint, slo, "3d", now_ts).await;

    let window_end = Utc::now();
    let window_start = window_end - ChronoDuration::days(slo.window_days as i64);

    sqlx::query(
        r#"INSERT INTO error_budget_snapshots
               (slo_id, window_start, window_end, sli_achieved_pct,
                budget_total_minutes, budget_consumed_minutes,
                budget_remaining_pct, burn_rate_1h, burn_rate_6h,
                burn_rate_24h, burn_rate_3d)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)"#,
    )
    .bind(slo.id)
    .bind(window_start)
    .bind(window_end)
    .bind(sli_achieved_pct)
    .bind(budget_total)
    .bind(budget_consumed)
    .bind(budget_remaining_pct)
    .bind(burn_1h)
    .bind(burn_6h)
    .bind(burn_24h)
    .bind(burn_3d)
    .execute(pool)
    .await?;

    tracing::debug!(
        slo_id = %slo.id,
        sli_pct = sli_achieved_pct,
        remaining_pct = budget_remaining_pct,
        "SLO snapshot recorded"
    );
    Ok(())
}

/// Build the PromQL for the SLI ratio over `window_days` for snapshot writes.
///
/// Prefers the W3-emitted recording rule (`sli:slo_<short>:ratio_rate3d` etc.)
/// when available — look-up is based on `recording_rules_hash` presence, not
/// the actual metric name, since Mimir will tell us via a null result if the
/// rule isn't installed yet.
fn sli_ratio_query(slo: &Slo, window_days: i32) -> String {
    let short = rule_generator::short_id(&slo.id);
    let prefix = format!("sli:slo_{}", short);

    // P1 #9: map window_days to the closest pre-aggregated recording rule
    // we emit (5m / 30m / 1h / 6h / 3d). Only 3d lines up cleanly with a
    // days-granularity SLO window; every other value falls through to the
    // raw aggregation path so the snapshot reflects the actual window rather
    // than being silently approximated to a coarser one.
    let window_suffix = match window_days {
        3 => Some("3d"),
        _ => None,
    };

    if slo.recording_rules_hash.is_some() {
        if let Some(suffix) = window_suffix {
            return format!("{prefix}:ratio_rate{suffix}");
        }
        // Recording rules present but window_days doesn't match a published
        // window — aggregate the 5m ratio over the requested window so the
        // snapshot matches the configured window exactly.
        return format!("avg_over_time({prefix}:ratio_rate5m[{window_days}d])");
    }

    // No recording rules yet → raw good/total over window.
    format!(
        "(sum_over_time(({good})[{window_days}d:5m]) / sum_over_time(({total})[{window_days}d:5m]))",
        good = slo.good_events_query,
        total = slo.total_events_query
    )
}

/// Query Mimir for the burn rate over a specific window. Returns `None` on
/// any failure (network, empty series, parse) so the snapshot column gets
/// stored as SQL NULL — downstream charts can render "no data".
async fn fetch_burn_rate(
    endpoint: &mimir_client::MetricsEndpoint,
    slo: &Slo,
    window: &str,
    now_ts: i64,
) -> Option<f64> {
    let query = burn_rate_query(slo, window);
    let response = mimir_client::query_instant(endpoint, &query, Some(now_ts))
        .await
        .ok()?;
    let ratio = mimir_client::first_scalar(&response)?;
    let clamped = ratio.clamp(0.0, 1.0);
    Some(budget_calc::burn_rate(clamped, slo.objective_pct))
}

/// PromQL for the SLI ratio over a specific short window used in burn-rate
/// computation. Uses recording rules when installed; otherwise rolls the raw
/// queries.
fn burn_rate_query(slo: &Slo, window: &str) -> String {
    let short = rule_generator::short_id(&slo.id);
    let prefix = format!("sli:slo_{}", short);

    // Only these windows exist as recording rules. 24h isn't one of them —
    // approximate via avg_over_time on the 5m base when available.
    let rule_window = matches!(window, "1h" | "6h" | "3d");
    if slo.recording_rules_hash.is_some() && rule_window {
        return format!("{prefix}:ratio_rate{window}");
    }

    if slo.recording_rules_hash.is_some() {
        return format!("avg_over_time({prefix}:ratio_rate5m[{window}])");
    }

    format!(
        "(sum_over_time(({good})[{window}:5m]) / sum_over_time(({total})[{window}:5m]))",
        good = slo.good_events_query,
        total = slo.total_events_query
    )
}

// ---------------------------------------------------------------------------
// Tests — no Mimir I/O; DB writes are integration-tested in W7. Unit tests
// here cover the PromQL builder, which is the highest risk of silent drift
// once recording rules are deployed.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use serde_json::json;
    use uuid::Uuid;

    fn sample_slo(with_rules: bool, window_days: i32) -> Slo {
        let id = Uuid::parse_str("7a3cbeef-1234-4567-89ab-abcd12345678").unwrap();
        Slo {
            id,
            tenant_id: Uuid::new_v4(),
            component_id: None,
            name: "test-slo".to_string(),
            description: None,
            sli_type: "availability".to_string(),
            good_events_query: "sum(rate(req_good[5m]))".to_string(),
            total_events_query: "sum(rate(req_total[5m]))".to_string(),
            objective_pct: 99.9,
            window_days,
            burn_rate_policy: "mwmbr_default".to_string(),
            labels: json!({}),
            enabled: true,
            recording_rules_hash: if with_rules {
                Some("abc123def456".to_string())
            } else {
                None
            },
            created_by: Uuid::new_v4(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn sli_ratio_query_uses_rule_for_3d_window_when_synced() {
        let slo = sample_slo(true, 3);
        let q = sli_ratio_query(&slo, slo.window_days);
        assert_eq!(q, "sli:slo_7a3cbeef:ratio_rate3d");
    }

    #[test]
    fn sli_ratio_query_uses_avg_over_time_for_1d_window() {
        // P1 #9: window_days=1 no longer silently collapses onto the 1h
        // ratio rule. Use avg_over_time on the 5m base so the snapshot
        // reflects the full 1d window.
        let slo = sample_slo(true, 1);
        let q = sli_ratio_query(&slo, slo.window_days);
        assert_eq!(q, "avg_over_time(sli:slo_7a3cbeef:ratio_rate5m[1d])");
    }

    #[test]
    fn sli_ratio_query_uses_avg_over_time_for_2d_window() {
        // window_days=2: no native rule, fall through to avg_over_time on
        // the 5m base.
        let slo = sample_slo(true, 2);
        let q = sli_ratio_query(&slo, slo.window_days);
        assert_eq!(q, "avg_over_time(sli:slo_7a3cbeef:ratio_rate5m[2d])");
    }

    #[test]
    fn sli_ratio_query_aggregates_for_large_window_when_synced() {
        // window_days=28 is beyond any published recording rule — fall back
        // to avg_over_time on the base 5m rule.
        let slo = sample_slo(true, 28);
        let q = sli_ratio_query(&slo, slo.window_days);
        assert!(q.contains("avg_over_time(sli:slo_7a3cbeef:ratio_rate5m[28d])"));
    }

    #[test]
    fn sli_ratio_query_falls_back_to_raw_without_rules() {
        let slo = sample_slo(false, 28);
        let q = sli_ratio_query(&slo, slo.window_days);
        assert!(q.contains("sum_over_time((sum(rate(req_good[5m])))[28d:5m])"));
        assert!(q.contains("sum_over_time((sum(rate(req_total[5m])))[28d:5m])"));
    }

    #[test]
    fn burn_rate_query_uses_recording_rule_for_short_windows() {
        let slo = sample_slo(true, 28);
        assert_eq!(burn_rate_query(&slo, "1h"), "sli:slo_7a3cbeef:ratio_rate1h");
        assert_eq!(burn_rate_query(&slo, "6h"), "sli:slo_7a3cbeef:ratio_rate6h");
        assert_eq!(burn_rate_query(&slo, "3d"), "sli:slo_7a3cbeef:ratio_rate3d");
    }

    #[test]
    fn burn_rate_query_24h_approximates_via_avg_over_time() {
        let slo = sample_slo(true, 28);
        let q = burn_rate_query(&slo, "24h");
        assert!(q.contains("avg_over_time(sli:slo_7a3cbeef:ratio_rate5m[24h])"));
    }

    #[test]
    fn burn_rate_query_falls_back_to_raw_without_rules() {
        let slo = sample_slo(false, 28);
        let q = burn_rate_query(&slo, "1h");
        assert!(q.contains("sum_over_time((sum(rate(req_good[5m])))[1h:5m])"));
    }
}
