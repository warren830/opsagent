//! Prediction scheduler — periodically checks CloudWatch Anomaly Detection
//! and Mimir predict_linear() for capacity/performance risks.
//! Creates `issue_type='prediction'` issues when anomalies are detected.

use anyhow::Result;
use aws_sdk_cloudwatch as cw;
use chrono::Utc;
use sqlx::PgPool;

use crate::services::alerts::upsert_issue;

/// Run a full prediction check cycle.
/// Called by the scheduler in main.rs every N seconds.
pub async fn run_prediction_check(pool: &PgPool) -> Result<()> {
    tracing::info!("Prediction check: starting cycle");

    // Run CloudWatch and Mimir checks concurrently
    let cw_handle = check_cloudwatch_anomalies(pool);
    let mimir_handle = check_mimir_predictions(pool);

    let (cw_result, mimir_result) = tokio::join!(cw_handle, mimir_handle);

    if let Err(e) = cw_result {
        tracing::error!("CloudWatch prediction check failed: {}", e);
    }
    if let Err(e) = mimir_result {
        tracing::error!("Mimir prediction check failed: {}", e);
    }

    tracing::info!("Prediction check: cycle complete");
    Ok(())
}

// ─── CloudWatch Anomaly Detection ────────────────────────────

async fn check_cloudwatch_anomalies(pool: &PgPool) -> Result<()> {
    let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    let client = cw::Client::new(&config);

    // List all configured anomaly detectors
    let detectors = client.describe_anomaly_detectors().send().await?;

    let detectors = detectors.anomaly_detectors();
    if detectors.is_empty() {
        tracing::debug!("No CloudWatch anomaly detectors configured, skipping");
        return Ok(());
    }

    tracing::info!("Found {} CloudWatch anomaly detectors", detectors.len());

    let now = Utc::now();
    let start = now - chrono::Duration::hours(1);

    for detector in detectors {
        // Use SingleMetricAnomalyDetector (non-deprecated API)
        let smad = match detector.single_metric_anomaly_detector() {
            Some(s) => s,
            None => continue,
        };
        let metric_name = match smad.metric_name() {
            Some(n) => n,
            None => continue,
        };
        let namespace = match smad.namespace() {
            Some(n) => n,
            None => continue,
        };
        let stat_str = smad.stat().unwrap_or("Average");

        // Build dimension info for dedup key and description
        let dims: Vec<(String, String)> = smad
            .dimensions()
            .iter()
            .filter_map(|d| {
                let name = d.name.as_deref()?;
                let value = d.value.as_deref()?;
                Some((name.to_string(), value.to_string()))
            })
            .collect();

        let dim_label = dims
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<_>>()
            .join(",");

        let primary_dim_value = dims.first().map(|(_, v)| v.as_str()).unwrap_or("unknown");

        // Query the metric WITH anomaly detection band
        let m1_stat = match stat_str {
            "Average" => cw::types::Statistic::Average,
            "Maximum" => cw::types::Statistic::Maximum,
            "Minimum" => cw::types::Statistic::Minimum,
            "Sum" => cw::types::Statistic::Sum,
            _ => cw::types::Statistic::Average,
        };

        let metric_dimensions: Vec<cw::types::Dimension> = dims
            .iter()
            .map(|(k, v)| cw::types::Dimension::builder().name(k).value(v).build())
            .collect();

        let metric_stat = cw::types::MetricStat::builder()
            .metric(
                cw::types::Metric::builder()
                    .namespace(namespace)
                    .metric_name(metric_name)
                    .set_dimensions(Some(metric_dimensions))
                    .build(),
            )
            .period(300)
            .stat(m1_stat.as_str())
            .build();

        let query_actual = cw::types::MetricDataQuery::builder()
            .id("m1")
            .metric_stat(metric_stat)
            .build();

        let query_band = cw::types::MetricDataQuery::builder()
            .id("ad1")
            .expression("ANOMALY_DETECTION_BAND(m1, 2)")
            .build();

        let response = client
            .get_metric_data()
            .metric_data_queries(query_actual)
            .metric_data_queries(query_band)
            .start_time(aws_sdk_cloudwatch::primitives::DateTime::from_millis(
                start.timestamp_millis(),
            ))
            .end_time(aws_sdk_cloudwatch::primitives::DateTime::from_millis(
                now.timestamp_millis(),
            ))
            .send()
            .await;

        let response = match response {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("Failed to query anomaly band for {}/{}: {}", namespace, metric_name, e);
                continue;
            }
        };

        // Parse results — check if actual value exceeds the anomaly band
        let results = response.metric_data_results();
        let actual_values: Vec<f64> = results
            .iter()
            .find(|r| r.id() == Some("m1"))
            .map(|r| r.values().to_vec())
            .unwrap_or_default();

        // The anomaly band returns upper and lower bounds interleaved
        // For simplicity, we check if the latest actual value exists and if we got band data
        let band_values: Vec<f64> = results
            .iter()
            .find(|r| r.id() == Some("ad1"))
            .map(|r| r.values().to_vec())
            .unwrap_or_default();

        if actual_values.is_empty() {
            continue;
        }

        let latest_actual = actual_values.last().copied().unwrap_or(0.0);

        // If we have band data, check upper bound (even indices = upper, odd = lower in some APIs)
        // Simplified: if band is available and actual > max band value, it's anomalous
        let is_anomalous = if !band_values.is_empty() {
            let max_band = band_values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            latest_actual > max_band
        } else {
            false
        };

        if !is_anomalous {
            continue;
        }

        // Determine severity based on how far outside the band
        let max_band = band_values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let deviation_pct = if max_band > 0.0 {
            ((latest_actual - max_band) / max_band * 100.0).round()
        } else {
            100.0
        };

        let severity = if deviation_pct > 50.0 {
            "critical"
        } else if deviation_pct > 20.0 {
            "high"
        } else {
            "medium"
        };

        let dedup_key = format!("prediction:cw:{}:{}:{}", namespace, metric_name, primary_dim_value);
        let title = format!(
            "[Prediction] {} / {} anomaly detected ({})",
            namespace, metric_name, dim_label
        );
        let description = format!(
            "CloudWatch Anomaly Detection: {}/{} ({}) current value {:.2} exceeds anomaly band upper bound {:.2} by {:.0}%",
            namespace, metric_name, dim_label, latest_actual, max_band, deviation_pct
        );

        let meta = serde_json::json!({
            "fingerprint": dedup_key,
            "source_type": "cloudwatch_anomaly",
            "namespace": namespace,
            "metric_name": metric_name,
            "dimensions": dim_label,
            "actual_value": latest_actual,
            "band_upper": max_band,
            "deviation_pct": deviation_pct,
        });

        let (created, _) = upsert_issue(
            pool,
            "cloudwatch",
            &dedup_key,
            &title,
            &description,
            severity,
            &meta,
            false,
            "prediction",
            None,
            None, // prediction jobs have no tenant context; row lands with NULL tenant_id
        )
        .await;

        if created > 0 {
            tracing::info!(
                "Created prediction issue: {}/{} ({}) — {:.0}% above band",
                namespace,
                metric_name,
                dim_label,
                deviation_pct
            );
        }
    }

    Ok(())
}

// ─── Mimir predict_linear() ──────────────────────────────────

/// Prediction queries to run against Mimir.
/// Each returns results only if the prediction threshold is breached.
const MIMIR_PREDICTION_QUERIES: &[(&str, &str, &str, &str)] = &[
    (
        "disk_full_4h",
        "predict_linear(node_filesystem_avail_bytes{fstype!=\"tmpfs\"}[1h], 14400) < 0",
        "Disk predicted to fill within 4 hours",
        "high",
    ),
    (
        "memory_oom_2h",
        "predict_linear(container_memory_working_set_bytes[30m], 7200) > container_spec_memory_limit_bytes",
        "Pod memory predicted to exceed limit within 2 hours",
        "high",
    ),
    (
        "cpu_saturated_1h",
        "predict_linear(rate(container_cpu_usage_seconds_total[5m])[30m:1m], 3600) > 0.9",
        "CPU predicted to saturate (>90%) within 1 hour",
        "medium",
    ),
];

async fn check_mimir_predictions(pool: &PgPool) -> Result<()> {
    // Read Mimir endpoint from telemetry_config (pick the first enabled config that routes metrics)
    let config = sqlx::query_as::<_, (String, serde_json::Value, bool)>(
        r#"SELECT provider, config, enabled FROM telemetry_config
           WHERE enabled = true
             AND routing->'signals' ? 'metrics'
           ORDER BY created_at DESC LIMIT 1"#,
    )
    .fetch_optional(pool)
    .await?;

    let Some((provider, config, true)) = config else {
        tracing::debug!("No enabled telemetry config with metrics routing, skipping Mimir predictions");
        return Ok(());
    };

    let mimir_url = config
        .get("mimir_endpoint_url")
        .or_else(|| config.get("mimir_endpoint"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if mimir_url.is_empty() {
        tracing::debug!("No Mimir endpoint configured, skipping Mimir predictions");
        return Ok(());
    }

    // Build auth header if cloud mode
    let is_cloud = provider != "self-hosted";
    let mimir_user = config.get("mimir_user_id").and_then(|v| v.as_str()).unwrap_or("");
    let api_token = config.get("api_token").and_then(|v| v.as_str()).unwrap_or("");

    let client = reqwest::Client::new();

    for (query_id, promql, description_template, default_severity) in MIMIR_PREDICTION_QUERIES {
        let url = format!("{}/api/v1/query", mimir_url);

        let mut req = client.post(&url).form(&[("query", *promql)]);

        if is_cloud && !mimir_user.is_empty() && !api_token.is_empty() {
            req = req.basic_auth(mimir_user, Some(api_token));
        }

        let response = match req.send().await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("Mimir query '{}' failed: {}", query_id, e);
                continue;
            }
        };

        let body: serde_json::Value = match response.json().await {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!("Mimir query '{}' bad response: {}", query_id, e);
                continue;
            }
        };

        // Parse Prometheus response: {"data":{"result":[...]}}
        let results = body
            .pointer("/data/result")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        if results.is_empty() {
            continue;
        }

        tracing::info!("Mimir prediction '{}' returned {} results", query_id, results.len());

        for result in &results {
            // Extract metric labels for identification
            let metric = result.get("metric").cloned().unwrap_or(serde_json::json!({}));
            let instance = metric
                .get("instance")
                .or_else(|| metric.get("pod"))
                .or_else(|| metric.get("node"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let namespace = metric.get("namespace").and_then(|v| v.as_str()).unwrap_or("default");

            let dedup_key = format!("prediction:mimir:{}:{}:{}", query_id, namespace, instance);
            let title = format!("[Prediction] {} — {}/{}", description_template, namespace, instance);
            let description = format!(
                "Mimir predict_linear: {} (namespace={}, instance={}). Query: {}",
                description_template, namespace, instance, promql
            );

            let meta = serde_json::json!({
                "fingerprint": dedup_key,
                "source_type": "mimir_prediction",
                "query_id": query_id,
                "promql": promql,
                "namespace": namespace,
                "instance": instance,
                "metric_labels": metric,
            });

            let (created, _) = upsert_issue(
                pool,
                "mimir",
                &dedup_key,
                &title,
                &description,
                default_severity,
                &meta,
                false,
                "prediction",
                None,
                None, // prediction jobs have no tenant context; row lands with NULL tenant_id
            )
            .await;

            if created > 0 {
                tracing::info!(
                    "Created Mimir prediction issue: {} ({}/{})",
                    query_id,
                    namespace,
                    instance
                );
            }
        }
    }

    Ok(())
}
