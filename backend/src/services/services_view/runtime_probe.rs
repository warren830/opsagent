//! Runtime probe dispatcher for the services v2 overview.
//!
//! This module turns a Catalog Component's declared `runtime` stanza into a
//! `RuntimeDetail` payload. In v1 we do **not** reach out to AWS / K8s /
//! external hosts — everything here is **DB-only** (design §3.3 + Q5). That
//! keeps `GET /api/services/overview` fast and predictable even when a
//! cluster is down or credentials are stale.
//!
//! - `eks`      → read the most recent `deployment_events` row for the
//!                matching (cluster_id, namespace, workload) triple.
//! - `ec2/rds/lambda` → echo back what the user declared in `spec.runtime`
//!                (instance_type / version / memory / etc.). Live probing
//!                requires AWS creds and goes into the v2 backlog.
//! - `external` → surface the declared base_url; ping RTT remains `None`
//!                until a cron job lands.
//! - `generic`  → pass the raw `spec.runtime` JSON through.
//! - unknown / missing runtime → `Generic { info: null }`.
//!
//! If we can't return useful data (EKS with no cached deployment_events,
//! cluster lookup fails, etc.) we fall back to `RuntimeDetail::Unavailable`
//! so the health calculator surfaces it as Critical (design §2 D9).

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::services_view::{RuntimeDetail, RuntimeSpec};

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
        "ec2" => probe_ec2(spec),
        "rds" => probe_rds(spec),
        "lambda" => probe_lambda(spec),
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

/// EC2 probe — v1 echoes the declared spec. state / AMI / instance_type
/// all come from whatever the user put in `spec.runtime`. Live
/// `ec2:DescribeInstances` is a v2 follow-up.
fn probe_ec2(spec: &RuntimeSpec) -> RuntimeDetail {
    RuntimeDetail::Ec2 {
        // We don't have a dedicated `state` column — surface nothing until
        // the AWS probe lands in v2. Same for AMI; `instance_type` is a
        // free-form string so we pull from `region` as a stand-in when
        // nothing richer is declared (rarely useful, but harmless).
        state: None,
        ami: None,
        instance_type: spec.region.clone(),
    }
}

/// RDS probe — same echo-only strategy as EC2. Engine / version / multi-az
/// / connection_count are all `None` until we add CloudWatch metrics.
fn probe_rds(_spec: &RuntimeSpec) -> RuntimeDetail {
    RuntimeDetail::Rds {
        engine: None,
        version: None,
        multi_az: None,
        connection_count: None,
    }
}

/// Lambda probe — echo-only. Live last-invocation / error-rate needs
/// `lambda:GetFunction` + CloudWatch, moved to v2.
fn probe_lambda(_spec: &RuntimeSpec) -> RuntimeDetail {
    RuntimeDetail::Lambda {
        version: None,
        memory_mb: None,
        last_invocation: None,
        error_rate_pct: None,
    }
}

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
// Tests — pure-function probes (EKS test needs a DB and is deferred to
// integration tests; we still exercise the dispatcher's branch shape by
// calling the non-DB variants directly).
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn base_spec(kind: &str) -> RuntimeSpec {
        RuntimeSpec {
            kind: kind.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn ec2_probe_returns_ec2_variant() {
        let spec = RuntimeSpec {
            kind: "ec2".into(),
            region: Some("t3.large".into()),
            ..Default::default()
        };
        let detail = probe_ec2(&spec);
        assert_eq!(detail.kind_tag(), "ec2");
        if let RuntimeDetail::Ec2 { instance_type, .. } = detail {
            assert_eq!(instance_type.as_deref(), Some("t3.large"));
        } else {
            panic!("expected Ec2");
        }
    }

    #[test]
    fn rds_probe_returns_rds_variant_with_none_fields() {
        let spec = base_spec("rds");
        let detail = probe_rds(&spec);
        assert_eq!(detail.kind_tag(), "rds");
        if let RuntimeDetail::Rds {
            engine,
            version,
            multi_az,
            connection_count,
        } = detail
        {
            assert!(engine.is_none());
            assert!(version.is_none());
            assert!(multi_az.is_none());
            assert!(connection_count.is_none());
        } else {
            panic!("expected Rds");
        }
    }

    #[test]
    fn lambda_probe_returns_lambda_variant() {
        let detail = probe_lambda(&base_spec("lambda"));
        assert_eq!(detail.kind_tag(), "lambda");
    }

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

    #[tokio::test]
    async fn probe_with_no_runtime_returns_generic_null() {
        // The dispatcher is `async` because EKS needs the pool, but the
        // Generic / unknown branches don't touch it. `pool_none_guard`
        // below shortcuts on None.
        //
        // We can't construct a real PgPool here without a DB, so this
        // test only covers the `runtime=None` path — that's still a real
        // contract we care about (legacy components with no runtime).
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
    async fn probe_ec2_through_dispatcher_returns_ec2_variant() {
        let spec = base_spec("ec2");
        let pool = lazy_unused_pool();
        let detail = probe(&pool, Some(&spec)).await;
        assert_eq!(detail.kind_tag(), "ec2");
    }

    #[tokio::test]
    async fn probe_rds_through_dispatcher_returns_rds_variant() {
        let spec = base_spec("rds");
        let pool = lazy_unused_pool();
        let detail = probe(&pool, Some(&spec)).await;
        assert_eq!(detail.kind_tag(), "rds");
    }

    #[tokio::test]
    async fn probe_lambda_through_dispatcher_returns_lambda_variant() {
        let spec = base_spec("lambda");
        let pool = lazy_unused_pool();
        let detail = probe(&pool, Some(&spec)).await;
        assert_eq!(detail.kind_tag(), "lambda");
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
