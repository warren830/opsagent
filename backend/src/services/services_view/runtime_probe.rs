//! Runtime probe dispatcher for the services v2 overview.
//!
//! This module turns a Catalog Component's declared `runtime` stanza into a
//! `RuntimeDetail` payload. v1.1 splits the probes into two classes:
//!
//! - `eks` → read the most recent `deployment_events` row for the
//!   matching (cluster_id, namespace, workload) triple. DB only.
//! - `external` → surface the declared base_url; ping RTT remains `None`
//!   until a cron job lands. DB only.
//! - `generic` → pass the raw `spec.runtime` JSON through.
//! - unknown / missing runtime → `Generic { info: null }`.
//! - `ec2` / `rds` / `lambda` → call the live AWS SDK (DescribeInstances,
//!   DescribeDBInstances, GetFunction). See the "credential strategy"
//!   section below.
//!
//! ## Credential strategy
//!
//! The AWS probes use the **default credential chain** (`aws-sdk-rust`
//! behaviour: env vars → shared config → IRSA / IMDS). This means the
//! backend probes whatever account its current credentials belong to — in
//! production that's the backend's IRSA role on EKS, in local dev it's
//! `~/.aws/credentials` / `$AWS_PROFILE`.
//!
//! **Known limitation**: resources declared on Components that live in a
//! different AWS account than the backend's IAM identity will return
//! `Unavailable { reason: "access denied" }`. Cross-account probing would
//! require an `AssumeRole` chain keyed on the resource's owning
//! `cloud_account.role_arn`; that's a v2 follow-up because it cuts across
//! schema lookups outside this file's scope.
//!
//! ## Client caching
//!
//! `aws_config::load_defaults(...)` costs ~100ms cold (resolves IMDS,
//! reads shared config). Under `GET /api/services/overview` we can call
//! the probes dozens of times in a single request, so we cache built
//! clients per region in module-level `OnceLock<RwLock<HashMap<_, _>>>`
//! maps. First call per region pays the cold-start cost; subsequent
//! calls reuse the client.
//!
//! ## Timeouts
//!
//! Every SDK call is wrapped in `tokio::time::timeout(10s)`. On timeout
//! we return `Unavailable { reason: "... timed out" }` rather than
//! blocking the whole overview request — the aggregator is a best-effort
//! rollup (design §2 D9).
//!
//! If we can't return useful data (missing field, resource not found,
//! auth failure, timeout, ...) we fall back to `RuntimeDetail::Unavailable`
//! so the health calculator surfaces it as Critical.

use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::Duration;

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use tokio::sync::RwLock;
use uuid::Uuid;

use aws_config::{BehaviorVersion, Region};
use aws_sdk_ec2::operation::describe_instances::DescribeInstancesOutput;
use aws_sdk_lambda::operation::get_function::GetFunctionOutput;
use aws_sdk_rds::operation::describe_db_instances::DescribeDbInstancesOutput;

use crate::models::services_view::{RuntimeDetail, RuntimeSpec};

/// Wall-clock budget for a single AWS SDK call. Longer than our p99
/// backend route SLO but short enough that a stalled API doesn't make
/// the whole overview hang.
const AWS_CALL_TIMEOUT: Duration = Duration::from_secs(10);

// ---------------------------------------------------------------------------
// Client caches — one per service, each keyed by region string.
//
// We can't cache an `SdkConfig` and derive clients cheaply, because the
// per-client builders also resolve regional endpoints. Caching the full
// client is the simplest correct thing.
// ---------------------------------------------------------------------------

static EC2_CLIENTS: OnceLock<RwLock<HashMap<String, aws_sdk_ec2::Client>>> =
    OnceLock::new();
static RDS_CLIENTS: OnceLock<RwLock<HashMap<String, aws_sdk_rds::Client>>> =
    OnceLock::new();
static LAMBDA_CLIENTS: OnceLock<RwLock<HashMap<String, aws_sdk_lambda::Client>>> =
    OnceLock::new();

fn ec2_cache() -> &'static RwLock<HashMap<String, aws_sdk_ec2::Client>> {
    EC2_CLIENTS.get_or_init(|| RwLock::new(HashMap::new()))
}

fn rds_cache() -> &'static RwLock<HashMap<String, aws_sdk_rds::Client>> {
    RDS_CLIENTS.get_or_init(|| RwLock::new(HashMap::new()))
}

fn lambda_cache() -> &'static RwLock<HashMap<String, aws_sdk_lambda::Client>> {
    LAMBDA_CLIENTS.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Build an `SdkConfig` targeting `region` using the default credential
/// chain. Kept private; each service-specific helper wraps this and
/// caches the resulting client.
async fn build_sdk_config(region: &str) -> aws_config::SdkConfig {
    aws_config::defaults(BehaviorVersion::latest())
        .region(Region::new(region.to_string()))
        .load()
        .await
}

async fn get_ec2_client(region: &str) -> aws_sdk_ec2::Client {
    {
        // Fast path — read lock, no rebuild.
        let guard = ec2_cache().read().await;
        if let Some(existing) = guard.get(region) {
            return existing.clone();
        }
    }
    let config = build_sdk_config(region).await;
    let client = aws_sdk_ec2::Client::new(&config);
    let mut guard = ec2_cache().write().await;
    // Another task may have raced us — defer to whichever arrived first.
    guard
        .entry(region.to_string())
        .or_insert_with(|| client)
        .clone()
}

async fn get_rds_client(region: &str) -> aws_sdk_rds::Client {
    {
        let guard = rds_cache().read().await;
        if let Some(existing) = guard.get(region) {
            return existing.clone();
        }
    }
    let config = build_sdk_config(region).await;
    let client = aws_sdk_rds::Client::new(&config);
    let mut guard = rds_cache().write().await;
    guard
        .entry(region.to_string())
        .or_insert_with(|| client)
        .clone()
}

async fn get_lambda_client(region: &str) -> aws_sdk_lambda::Client {
    {
        let guard = lambda_cache().read().await;
        if let Some(existing) = guard.get(region) {
            return existing.clone();
        }
    }
    let config = build_sdk_config(region).await;
    let client = aws_sdk_lambda::Client::new(&config);
    let mut guard = lambda_cache().write().await;
    guard
        .entry(region.to_string())
        .or_insert_with(|| client)
        .clone()
}

// ---------------------------------------------------------------------------
// Dispatcher
// ---------------------------------------------------------------------------

/// Probe a single Component. Returns `Unavailable` (not an error) when
/// the data isn't there — the aggregator is a read-only rollup and should
/// keep going rather than fail the whole page.
pub async fn probe(pool: &PgPool, runtime: Option<&RuntimeSpec>) -> RuntimeDetail {
    let Some(spec) = runtime else {
        return RuntimeDetail::Generic {
            info: serde_json::Value::Null,
        };
    };

    match spec.kind.as_str() {
        "eks" => probe_eks(pool, spec).await,
        "ec2" => probe_ec2(spec).await,
        "rds" => probe_rds(spec).await,
        "lambda" => probe_lambda(spec).await,
        "external" => probe_external(spec),
        "generic" | "" => RuntimeDetail::Generic {
            info: serde_json::to_value(spec).unwrap_or(serde_json::Value::Null),
        },
        _ => {
            // Unknown kind — treat it like generic so the UI falls back
            // to the default card renderer rather than breaking.
            RuntimeDetail::Generic {
                info: serde_json::to_value(spec).unwrap_or(serde_json::Value::Null),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// EKS probe — DB only (unchanged from v1)
// ---------------------------------------------------------------------------

/// EKS probe — v1 reads `deployment_events` (the Argo Rollouts audit log
/// the watcher already maintains) instead of hitting the live K8s API.
///
/// Strategy:
/// 1. Resolve `cluster_id` from `clusters.name = spec.cluster` (tenant
///    agnostic; names are globally unique in our schema).
/// 2. SELECT the latest deployment_events row for (cluster_id, namespace,
///    rollout_name=workload). The `detail` JSONB typically has
///    `replicas_ready`, `replicas_desired`, `image` fields set by the
///    rollout watcher.
/// 3. If any step fails or data is missing, return Unavailable with a
///    specific reason string — useful in the UI tooltip.
async fn probe_eks(pool: &PgPool, spec: &RuntimeSpec) -> RuntimeDetail {
    let Some(cluster_name) = spec.cluster.as_deref() else {
        return RuntimeDetail::Unavailable {
            reason: "runtime.cluster not set".into(),
        };
    };
    let Some(namespace) = spec.namespace.as_deref() else {
        return RuntimeDetail::Unavailable {
            reason: "runtime.namespace not set".into(),
        };
    };
    let Some(workload) = spec.workload.as_deref() else {
        return RuntimeDetail::Unavailable {
            reason: "runtime.workload not set".into(),
        };
    };

    let cluster_row: Result<Option<(Uuid,)>, sqlx::Error> =
        sqlx::query_as("SELECT id FROM clusters WHERE name = $1 LIMIT 1")
            .bind(cluster_name)
            .fetch_optional(pool)
            .await;

    let cluster_id = match cluster_row {
        Ok(Some((id,))) => id,
        Ok(None) => {
            return RuntimeDetail::Unavailable {
                reason: format!("cluster '{cluster_name}' not registered"),
            };
        }
        Err(e) => {
            tracing::warn!("cluster lookup failed for {cluster_name}: {e}");
            return RuntimeDetail::Unavailable {
                reason: "cluster lookup failed".into(),
            };
        }
    };

    // Latest deployment event for this (cluster, ns, workload). `action`
    // is not filtered — any event counts as evidence of a rollout.
    let event_row: Result<
        Option<(serde_json::Value, DateTime<Utc>)>,
        sqlx::Error,
    > = sqlx::query_as(
        r#"SELECT detail, created_at
           FROM deployment_events
           WHERE cluster_id = $1 AND namespace = $2 AND rollout_name = $3
           ORDER BY created_at DESC
           LIMIT 1"#,
    )
    .bind(cluster_id)
    .bind(namespace)
    .bind(workload)
    .fetch_optional(pool)
    .await;

    match event_row {
        Ok(Some((detail, ts))) => {
            // Pull the common fields — the rollout watcher stores them
            // under predictable keys, but callers may upsert rows with
            // different shapes, so every field is a `get().and_then()`
            // chain that tolerates absence.
            let pod_ready = detail
                .get("replicas_ready")
                .and_then(|v| v.as_i64())
                .map(|n| n as i32);
            let pod_desired = detail
                .get("replicas_desired")
                .and_then(|v| v.as_i64())
                .map(|n| n as i32);
            let image = detail
                .get("image")
                .and_then(|v| v.as_str())
                .map(String::from);

            RuntimeDetail::Eks {
                pod_ready,
                pod_desired,
                image,
                last_updated: Some(ts),
            }
        }
        Ok(None) => RuntimeDetail::Unavailable {
            reason: format!(
                "no deployment_events for {cluster_name}/{namespace}/{workload}"
            ),
        },
        Err(e) => {
            tracing::warn!(
                "deployment_events lookup failed for {cluster_name}/{namespace}/{workload}: {e}"
            );
            RuntimeDetail::Unavailable {
                reason: "deployment_events lookup failed".into(),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// EC2 probe — live `ec2:DescribeInstances`
// ---------------------------------------------------------------------------

/// EC2 probe — calls `ec2:DescribeInstances` via the default credential
/// chain and returns `state`, `ami`, and `instance_type` for the single
/// instance identified by `spec.instance_id`.
async fn probe_ec2(spec: &RuntimeSpec) -> RuntimeDetail {
    let Some(instance_id) = spec.instance_id.as_deref() else {
        return RuntimeDetail::Unavailable {
            reason: "runtime.instance_id not set".into(),
        };
    };
    let Some(region) = spec.region.as_deref() else {
        return RuntimeDetail::Unavailable {
            reason: "runtime.region not set".into(),
        };
    };

    let client = get_ec2_client(region).await;
    let call = client.describe_instances().instance_ids(instance_id).send();

    let output = match tokio::time::timeout(AWS_CALL_TIMEOUT, call).await {
        Ok(Ok(out)) => out,
        Ok(Err(e)) => {
            tracing::warn!(
                region = region,
                instance_id = instance_id,
                "ec2 DescribeInstances failed: {}",
                aws_sdk_ec2::error::DisplayErrorContext(&e),
            );
            return RuntimeDetail::Unavailable {
                reason: summarise_sdk_error("ec2 describe_instances", &e),
            };
        }
        Err(_) => {
            return RuntimeDetail::Unavailable {
                reason: "ec2 describe_instances timed out".into(),
            };
        }
    };

    build_ec2_detail_from_response(&output, instance_id)
}

/// Pure helper used by the live probe and unit tests. Takes a decoded
/// `DescribeInstancesOutput` and returns the `RuntimeDetail` the caller
/// should surface. Keeps the AWS I/O out of unit-test scope.
fn build_ec2_detail_from_response(
    output: &DescribeInstancesOutput,
    instance_id: &str,
) -> RuntimeDetail {
    let instance = output
        .reservations()
        .iter()
        .flat_map(|r| r.instances())
        .next();

    let Some(instance) = instance else {
        return RuntimeDetail::Unavailable {
            reason: format!("instance not found: {instance_id}"),
        };
    };

    let state = instance
        .state()
        .and_then(|s| s.name())
        .map(|n| n.as_str().to_string());
    let ami = instance.image_id().map(String::from);
    let instance_type = instance
        .instance_type()
        .map(|t| t.as_str().to_string());

    RuntimeDetail::Ec2 {
        state,
        ami,
        instance_type,
    }
}

// ---------------------------------------------------------------------------
// RDS probe — live `rds:DescribeDBInstances`
// ---------------------------------------------------------------------------

/// RDS probe — calls `rds:DescribeDBInstances` and surfaces engine,
/// version, and multi-AZ status. `connection_count` stays `None` because
/// it needs a CloudWatch `DatabaseConnections` metric call (out of scope
/// for this PR; see TODO).
async fn probe_rds(spec: &RuntimeSpec) -> RuntimeDetail {
    let Some(db_identifier) = spec.instance_id.as_deref() else {
        return RuntimeDetail::Unavailable {
            reason: "runtime.instance_id not set".into(),
        };
    };
    let Some(region) = spec.region.as_deref() else {
        return RuntimeDetail::Unavailable {
            reason: "runtime.region not set".into(),
        };
    };

    let client = get_rds_client(region).await;
    let call = client
        .describe_db_instances()
        .db_instance_identifier(db_identifier)
        .send();

    let output = match tokio::time::timeout(AWS_CALL_TIMEOUT, call).await {
        Ok(Ok(out)) => out,
        Ok(Err(e)) => {
            tracing::warn!(
                region = region,
                db_identifier = db_identifier,
                "rds DescribeDBInstances failed: {}",
                aws_sdk_rds::error::DisplayErrorContext(&e),
            );
            return RuntimeDetail::Unavailable {
                reason: summarise_sdk_error("rds describe_db_instances", &e),
            };
        }
        Err(_) => {
            return RuntimeDetail::Unavailable {
                reason: "rds describe_db_instances timed out".into(),
            };
        }
    };

    build_rds_detail_from_response(&output, db_identifier)
}

/// Pure helper — converts a `DescribeDbInstancesOutput` to a
/// `RuntimeDetail::Rds`. TODO: add CloudWatch `DatabaseConnections` read
/// to fill `connection_count`; requires `aws-sdk-cloudwatch` + a metric
/// query, which is out of scope for this PR.
fn build_rds_detail_from_response(
    output: &DescribeDbInstancesOutput,
    db_identifier: &str,
) -> RuntimeDetail {
    let Some(db) = output.db_instances().iter().next() else {
        return RuntimeDetail::Unavailable {
            reason: format!("db instance not found: {db_identifier}"),
        };
    };

    let engine = db.engine().map(String::from);
    let version = db.engine_version().map(String::from);
    let multi_az = db.multi_az();

    RuntimeDetail::Rds {
        engine,
        version,
        multi_az,
        // TODO(v2): fetch CloudWatch DatabaseConnections metric.
        connection_count: None,
    }
}

// ---------------------------------------------------------------------------
// Lambda probe — live `lambda:GetFunction`
// ---------------------------------------------------------------------------

/// Lambda probe — calls `lambda:GetFunction` with either an ARN or a
/// bare function name. `last_invocation` and `error_rate_pct` need
/// CloudWatch Logs / Metrics integration and stay `None` for now.
async fn probe_lambda(spec: &RuntimeSpec) -> RuntimeDetail {
    let Some(arn_or_name) = spec.arn.as_deref() else {
        return RuntimeDetail::Unavailable {
            reason: "runtime.arn not set".into(),
        };
    };
    // Region defaults to whatever the default config picks up (ARNs
    // carry the region, but the SDK client still needs a region for
    // signing). If the caller didn't specify it, fall back to the env /
    // profile default by building a client in us-east-1 and letting AWS
    // pick — but it's cleaner to require region explicitly so cache
    // keys stay stable. Require it.
    let Some(region) = spec.region.as_deref() else {
        return RuntimeDetail::Unavailable {
            reason: "runtime.region not set".into(),
        };
    };

    let client = get_lambda_client(region).await;
    let call = client
        .get_function()
        .function_name(arn_or_name)
        .send();

    let output = match tokio::time::timeout(AWS_CALL_TIMEOUT, call).await {
        Ok(Ok(out)) => out,
        Ok(Err(e)) => {
            tracing::warn!(
                region = region,
                function = arn_or_name,
                "lambda GetFunction failed: {}",
                aws_sdk_lambda::error::DisplayErrorContext(&e),
            );
            return RuntimeDetail::Unavailable {
                reason: summarise_sdk_error("lambda get_function", &e),
            };
        }
        Err(_) => {
            return RuntimeDetail::Unavailable {
                reason: "lambda get_function timed out".into(),
            };
        }
    };

    build_lambda_detail_from_response(&output)
}

/// Pure helper — converts a `GetFunctionOutput` to a
/// `RuntimeDetail::Lambda`. TODO(v2): derive `last_invocation` from
/// CloudWatch Logs Insights and `error_rate_pct` from CloudWatch
/// `Errors` / `Invocations` metrics.
fn build_lambda_detail_from_response(output: &GetFunctionOutput) -> RuntimeDetail {
    let Some(config) = output.configuration() else {
        return RuntimeDetail::Unavailable {
            reason: "lambda response missing configuration".into(),
        };
    };

    let version = config.version().map(String::from);
    let memory_mb = config.memory_size();

    RuntimeDetail::Lambda {
        version,
        memory_mb,
        // TODO(v2): CloudWatch Logs Insights for last invocation.
        last_invocation: None,
        // TODO(v2): CloudWatch metrics for error rate.
        error_rate_pct: None,
    }
}

// ---------------------------------------------------------------------------
// External probe — declared URL only
// ---------------------------------------------------------------------------

/// External probe — surface the base_url so the card can at least render
/// the endpoint. `last_rtt_ms` / `last_check` stay `None` until a
/// scheduled synthetic ping job writes to a dedicated table.
fn probe_external(spec: &RuntimeSpec) -> RuntimeDetail {
    RuntimeDetail::External {
        base_url: spec.base_url.clone(),
        last_rtt_ms: None,
        last_check: None,
    }
}

// ---------------------------------------------------------------------------
// Error formatting
// ---------------------------------------------------------------------------

/// Produce a short, UI-friendly reason string from an SDK error. The
/// full `Debug` dump gets logged elsewhere; this one lands in the
/// `Unavailable.reason` tooltip so it has to stay readable.
fn summarise_sdk_error<E: std::error::Error>(op: &str, err: &E) -> String {
    // Walk one level of the source chain — SdkError wraps the service
    // error which has the AWS code; that's what users care about.
    let top = err.to_string();
    if let Some(source) = err.source() {
        let sub = source.to_string();
        // Many AWS errors print like "service error" with the code as
        // the source; combine them but cap the length so the tooltip
        // doesn't explode.
        let combined = format!("{op}: {top}: {sub}");
        return truncate(&combined, 160);
    }
    truncate(&format!("{op}: {top}"), 160)
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        let mut out = s[..max].to_string();
        out.push('…');
        out
    }
}

// ---------------------------------------------------------------------------
// Tests
//
// The live AWS paths can't be exercised without network + credentials, so
// we fabricate AWS response structs and test the pure `build_*_detail_*`
// helpers, plus the dispatcher's missing-field branches which don't
// need AWS at all.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use aws_sdk_ec2::types::{Instance, InstanceState, InstanceStateName, InstanceType, Reservation};
    use aws_sdk_lambda::types::FunctionConfiguration;
    use aws_sdk_rds::types::DbInstance;

    fn base_spec(kind: &str) -> RuntimeSpec {
        RuntimeSpec {
            kind: kind.to_string(),
            ..Default::default()
        }
    }

    // ---- probe_external / EKS-adjacent remains synchronous ------------

    #[test]
    fn external_probe_surfaces_base_url() {
        let spec = RuntimeSpec {
            kind: "external".into(),
            base_url: Some("https://api.partner.example".into()),
            ..Default::default()
        };
        let detail = probe_external(&spec);
        assert_eq!(detail.kind_tag(), "external");
        if let RuntimeDetail::External { base_url, .. } = detail {
            assert_eq!(base_url.as_deref(), Some("https://api.partner.example"));
        } else {
            panic!("expected External");
        }
    }

    // ---- EC2 probe: missing-field branches ----------------------------

    #[tokio::test]
    async fn ec2_probe_missing_instance_id_is_unavailable() {
        let spec = RuntimeSpec {
            kind: "ec2".into(),
            region: Some("us-west-2".into()),
            ..Default::default()
        };
        let detail = probe_ec2(&spec).await;
        assert_eq!(detail.kind_tag(), "unavailable");
        if let RuntimeDetail::Unavailable { reason } = detail {
            assert!(reason.contains("instance_id"));
        } else {
            panic!("expected Unavailable");
        }
    }

    #[tokio::test]
    async fn ec2_probe_missing_region_is_unavailable() {
        let spec = RuntimeSpec {
            kind: "ec2".into(),
            instance_id: Some("i-0123".into()),
            ..Default::default()
        };
        let detail = probe_ec2(&spec).await;
        assert_eq!(detail.kind_tag(), "unavailable");
        if let RuntimeDetail::Unavailable { reason } = detail {
            assert!(reason.contains("region"));
        } else {
            panic!("expected Unavailable");
        }
    }

    // ---- EC2 pure helper over fabricated SDK response ------------------

    #[test]
    fn ec2_build_detail_from_populated_response_maps_fields() {
        let instance = Instance::builder()
            .image_id("ami-abc123")
            .instance_type(InstanceType::T3Large)
            .state(
                InstanceState::builder()
                    .name(InstanceStateName::Running)
                    .build(),
            )
            .build();
        let reservation = Reservation::builder().instances(instance).build();
        let output = DescribeInstancesOutput::builder()
            .reservations(reservation)
            .build();

        let detail = build_ec2_detail_from_response(&output, "i-0123");
        match detail {
            RuntimeDetail::Ec2 {
                state,
                ami,
                instance_type,
            } => {
                assert_eq!(state.as_deref(), Some("running"));
                assert_eq!(ami.as_deref(), Some("ami-abc123"));
                assert_eq!(instance_type.as_deref(), Some("t3.large"));
            }
            _ => panic!("expected Ec2"),
        }
    }

    #[test]
    fn ec2_build_detail_from_empty_response_is_unavailable() {
        // 0 reservations → "instance not found".
        let output = DescribeInstancesOutput::builder().build();
        let detail = build_ec2_detail_from_response(&output, "i-missing");
        assert_eq!(detail.kind_tag(), "unavailable");
        if let RuntimeDetail::Unavailable { reason } = detail {
            assert!(
                reason.contains("i-missing"),
                "reason should name the missing instance: {reason}"
            );
        } else {
            panic!("expected Unavailable");
        }
    }

    #[test]
    fn ec2_build_detail_from_reservation_without_instances_is_unavailable() {
        // A reservation with no instances array — still "not found".
        let reservation = Reservation::builder().build();
        let output = DescribeInstancesOutput::builder()
            .reservations(reservation)
            .build();
        let detail = build_ec2_detail_from_response(&output, "i-x");
        assert_eq!(detail.kind_tag(), "unavailable");
    }

    // ---- RDS probe: missing-field branches ----------------------------

    #[tokio::test]
    async fn rds_probe_missing_instance_id_is_unavailable() {
        let spec = RuntimeSpec {
            kind: "rds".into(),
            region: Some("us-west-2".into()),
            ..Default::default()
        };
        let detail = probe_rds(&spec).await;
        assert_eq!(detail.kind_tag(), "unavailable");
    }

    #[tokio::test]
    async fn rds_probe_missing_region_is_unavailable() {
        let spec = RuntimeSpec {
            kind: "rds".into(),
            instance_id: Some("orders-db".into()),
            ..Default::default()
        };
        let detail = probe_rds(&spec).await;
        assert_eq!(detail.kind_tag(), "unavailable");
    }

    // ---- RDS pure helper -----------------------------------------------

    #[test]
    fn rds_build_detail_from_populated_response_maps_fields() {
        let db = DbInstance::builder()
            .db_instance_identifier("orders-db")
            .engine("postgres")
            .engine_version("15.4")
            .multi_az(true)
            .build();
        let output = DescribeDbInstancesOutput::builder()
            .db_instances(db)
            .build();

        let detail = build_rds_detail_from_response(&output, "orders-db");
        match detail {
            RuntimeDetail::Rds {
                engine,
                version,
                multi_az,
                connection_count,
            } => {
                assert_eq!(engine.as_deref(), Some("postgres"));
                assert_eq!(version.as_deref(), Some("15.4"));
                assert_eq!(multi_az, Some(true));
                // CloudWatch integration is TODO — field stays None.
                assert!(connection_count.is_none());
            }
            _ => panic!("expected Rds"),
        }
    }

    #[test]
    fn rds_build_detail_from_empty_response_is_unavailable() {
        let output = DescribeDbInstancesOutput::builder().build();
        let detail = build_rds_detail_from_response(&output, "orders-db");
        assert_eq!(detail.kind_tag(), "unavailable");
        if let RuntimeDetail::Unavailable { reason } = detail {
            assert!(reason.contains("orders-db"));
        } else {
            panic!("expected Unavailable");
        }
    }

    // ---- Lambda probe: missing-field branches -------------------------

    #[tokio::test]
    async fn lambda_probe_missing_arn_is_unavailable() {
        let spec = RuntimeSpec {
            kind: "lambda".into(),
            region: Some("us-west-2".into()),
            ..Default::default()
        };
        let detail = probe_lambda(&spec).await;
        assert_eq!(detail.kind_tag(), "unavailable");
        if let RuntimeDetail::Unavailable { reason } = detail {
            assert!(reason.contains("arn"));
        } else {
            panic!("expected Unavailable");
        }
    }

    #[tokio::test]
    async fn lambda_probe_missing_region_is_unavailable() {
        let spec = RuntimeSpec {
            kind: "lambda".into(),
            arn: Some("arn:aws:lambda:us-west-2:123:function:foo".into()),
            ..Default::default()
        };
        let detail = probe_lambda(&spec).await;
        assert_eq!(detail.kind_tag(), "unavailable");
    }

    // ---- Lambda pure helper --------------------------------------------

    #[test]
    fn lambda_build_detail_from_populated_response_maps_fields() {
        let config = FunctionConfiguration::builder()
            .function_name("foo")
            .version("7")
            .memory_size(512)
            .build();
        let output = GetFunctionOutput::builder().configuration(config).build();
        let detail = build_lambda_detail_from_response(&output);
        match detail {
            RuntimeDetail::Lambda {
                version,
                memory_mb,
                last_invocation,
                error_rate_pct,
            } => {
                assert_eq!(version.as_deref(), Some("7"));
                assert_eq!(memory_mb, Some(512));
                assert!(last_invocation.is_none());
                assert!(error_rate_pct.is_none());
            }
            _ => panic!("expected Lambda"),
        }
    }

    #[test]
    fn lambda_build_detail_from_response_missing_configuration_is_unavailable() {
        let output = GetFunctionOutput::builder().build();
        let detail = build_lambda_detail_from_response(&output);
        assert_eq!(detail.kind_tag(), "unavailable");
    }

    // ---- Error summarisation -------------------------------------------

    #[test]
    fn truncate_returns_short_string_unchanged() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn truncate_caps_long_string_with_ellipsis() {
        let got = truncate(&"x".repeat(200), 10);
        assert_eq!(got.chars().count(), 11); // 10 chars + '…'
        assert!(got.ends_with('…'));
    }

    #[test]
    fn summarise_sdk_error_prefixes_operation_name() {
        use std::io;
        let err = io::Error::new(io::ErrorKind::Other, "boom");
        let reason = summarise_sdk_error("rds describe_db_instances", &err);
        assert!(reason.contains("rds describe_db_instances"));
        assert!(reason.contains("boom"));
    }

    // ---- Dispatcher smoke tests ----------------------------------------

    #[tokio::test]
    async fn probe_with_no_runtime_returns_generic_null() {
        let pool = lazy_unused_pool();
        let detail = probe(&pool, None).await;
        assert_eq!(detail.kind_tag(), "generic");
        if let RuntimeDetail::Generic { info } = detail {
            assert_eq!(info, serde_json::Value::Null);
        } else {
            panic!("expected Generic");
        }
    }

    #[tokio::test]
    async fn probe_generic_kind_echoes_spec() {
        let spec = RuntimeSpec {
            kind: "generic".into(),
            base_url: Some("sqs://queue.example".into()),
            ..Default::default()
        };
        let pool = lazy_unused_pool();
        let detail = probe(&pool, Some(&spec)).await;
        assert_eq!(detail.kind_tag(), "generic");
    }

    #[tokio::test]
    async fn probe_unknown_kind_falls_back_to_generic() {
        let spec = base_spec("step_functions");
        let pool = lazy_unused_pool();
        let detail = probe(&pool, Some(&spec)).await;
        assert_eq!(detail.kind_tag(), "generic");
    }

    #[tokio::test]
    async fn probe_ec2_through_dispatcher_without_aws_fields_is_unavailable() {
        // ec2 kind but no instance_id → Unavailable, not Ec2 — we no
        // longer fabricate an empty Ec2 variant when the spec is too
        // thin for a probe.
        let spec = base_spec("ec2");
        let pool = lazy_unused_pool();
        let detail = probe(&pool, Some(&spec)).await;
        assert_eq!(detail.kind_tag(), "unavailable");
    }

    #[tokio::test]
    async fn probe_rds_through_dispatcher_without_aws_fields_is_unavailable() {
        let spec = base_spec("rds");
        let pool = lazy_unused_pool();
        let detail = probe(&pool, Some(&spec)).await;
        assert_eq!(detail.kind_tag(), "unavailable");
    }

    #[tokio::test]
    async fn probe_lambda_through_dispatcher_without_aws_fields_is_unavailable() {
        let spec = base_spec("lambda");
        let pool = lazy_unused_pool();
        let detail = probe(&pool, Some(&spec)).await;
        assert_eq!(detail.kind_tag(), "unavailable");
    }

    #[tokio::test]
    async fn probe_external_through_dispatcher_returns_external_variant() {
        let spec = RuntimeSpec {
            kind: "external".into(),
            base_url: Some("https://x".into()),
            ..Default::default()
        };
        let pool = lazy_unused_pool();
        let detail = probe(&pool, Some(&spec)).await;
        assert_eq!(detail.kind_tag(), "external");
    }

    /// `PgPool` handle that is never actually connected to. We use it only
    /// to satisfy the `probe` signature for branches that never hit the
    /// pool. The underlying TCP connect would fail if the branch did
    /// touch the DB — treat that as the test catching a regression.
    fn lazy_unused_pool() -> sqlx::PgPool {
        sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect_lazy("postgres://nobody:nobody@127.0.0.1:1/nobody")
            .expect("lazy pool")
    }
}
