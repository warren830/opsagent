//! Business logic for Argo Rollouts management.
//!
//! Extracted from `handlers::rollout` — all K8s CRD operations, DB queries,
//! and permission checks live here. Handlers are thin HTTP wrappers.

use kube::{
    api::{Api, DynamicObject, Patch, PatchParams},
    discovery::ApiResource,
};
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::middleware::auth::AuthUser;
use crate::models::deployment_event::DeploymentEvent;
use crate::models::rollout::{
    AnalysisRunSummary, ChangeStrategyRequest, RolloutDetail, RolloutSummary,
    parse_analysis_run, parse_canary_steps, parse_containers, parse_rollout_summary,
};
use crate::services::k8s::{build_k8s_client, load_and_authorize_cluster};

// ─── Argo Rollouts CRD ApiResource ───────────────────────────────────────────

pub fn rollout_api_resource() -> ApiResource {
    ApiResource {
        group: "argoproj.io".to_string(),
        version: "v1alpha1".to_string(),
        api_version: "argoproj.io/v1alpha1".to_string(),
        kind: "Rollout".to_string(),
        plural: "rollouts".to_string(),
    }
}

fn analysis_run_api_resource() -> ApiResource {
    ApiResource {
        group: "argoproj.io".to_string(),
        version: "v1alpha1".to_string(),
        api_version: "argoproj.io/v1alpha1".to_string(),
        kind: "AnalysisRun".to_string(),
        plural: "analysisruns".to_string(),
    }
}

// ─── List Rollouts ──────────────────────────────────────────────────────────

/// List all Argo Rollouts across all namespaces in a cluster.
pub async fn list_rollouts(
    pool: &PgPool,
    auth_user: &AuthUser,
    cluster_id: Uuid,
) -> AppResult<Vec<RolloutSummary>> {
    let cluster = load_and_authorize_cluster(pool, cluster_id, auth_user).await?;
    let client = build_k8s_client(pool, &cluster).await?;

    let ar = rollout_api_resource();
    let api: Api<DynamicObject> = Api::all_with(client, &ar);

    let list = api
        .list(&Default::default())
        .await
        .map_err(|e| AppError::Kubernetes(format!("List rollouts: {e}")))?;

    let rollouts: Vec<RolloutSummary> = list
        .items
        .iter()
        .filter_map(|obj| {
            let raw = serde_json::to_value(obj).ok()?;
            parse_rollout_summary(&raw)
        })
        .collect();

    Ok(rollouts)
}

// ─── Get Rollout Detail ─────────────────────────────────────────────────────

/// Get detailed info for a specific Rollout.
pub async fn get_rollout(
    pool: &PgPool,
    auth_user: &AuthUser,
    cluster_id: Uuid,
    ns: &str,
    name: &str,
) -> AppResult<RolloutDetail> {
    let cluster = load_and_authorize_cluster(pool, cluster_id, auth_user).await?;
    let client = build_k8s_client(pool, &cluster).await?;

    let ar = rollout_api_resource();
    let api: Api<DynamicObject> = Api::namespaced_with(client, ns, &ar);

    let obj = api
        .get(name)
        .await
        .map_err(|e| AppError::Kubernetes(format!("Get rollout {}/{}: {e}", ns, name)))?;

    let raw = serde_json::to_value(&obj)
        .map_err(|e| AppError::Internal(format!("Serialize rollout: {e}")))?;

    let summary =
        parse_rollout_summary(&raw).ok_or_else(|| AppError::Internal("Failed to parse rollout".to_string()))?;

    let current_step = summary.current_step;
    let canary_steps = parse_canary_steps(&raw, current_step);
    let containers = parse_containers(&raw);

    Ok(RolloutDetail {
        summary,
        canary_steps,
        containers,
    })
}

// ─── List Analysis Runs ─────────────────────────────────────────────────────

/// List AnalysisRuns associated with a Rollout (by ownerReference).
pub async fn list_analysis_runs(
    pool: &PgPool,
    auth_user: &AuthUser,
    cluster_id: Uuid,
    ns: &str,
    name: &str,
) -> AppResult<Vec<AnalysisRunSummary>> {
    let cluster = load_and_authorize_cluster(pool, cluster_id, auth_user).await?;
    let client = build_k8s_client(pool, &cluster).await?;

    let ar = analysis_run_api_resource();
    let api: Api<DynamicObject> = Api::namespaced_with(client, ns, &ar);

    let list = api
        .list(&Default::default())
        .await
        .map_err(|e| AppError::Kubernetes(format!("List analysis runs: {e}")))?;

    // Filter by ownerReference matching the rollout name
    let runs: Vec<AnalysisRunSummary> = list
        .items
        .iter()
        .filter(|obj| {
            obj.metadata
                .owner_references
                .as_ref()
                .map(|refs| refs.iter().any(|r| r.name == name && r.kind == "Rollout"))
                .unwrap_or(false)
        })
        .filter_map(|obj| {
            let raw = serde_json::to_value(obj).ok()?;
            parse_analysis_run(&raw)
        })
        .collect();

    Ok(runs)
}

// ─── Promote ────────────────────────────────────────────────────────────────

/// Promote a paused Rollout (advance one step or full promotion).
pub async fn promote(
    pool: &PgPool,
    auth_user: &AuthUser,
    cluster_id: Uuid,
    ns: &str,
    name: &str,
    full: bool,
) -> AppResult<serde_json::Value> {
    let cluster = load_and_authorize_cluster(pool, cluster_id, auth_user).await?;

    // Check write permission via account access
    check_write_permission(pool, auth_user, &cluster).await?;

    let client = build_k8s_client(pool, &cluster).await?;
    let ar = rollout_api_resource();
    let api: Api<DynamicObject> = Api::namespaced_with(client, ns, &ar);

    // Promote by clearing pause conditions
    // For full promotion, also set status.promoteFull = true
    let patch = if full {
        serde_json::json!({
            "status": {
                "pauseConditions": null,
                "controllerPause": false,
                "promoteFull": true
            }
        })
    } else {
        serde_json::json!({
            "status": {
                "pauseConditions": null,
                "controllerPause": false
            }
        })
    };

    let pp = PatchParams::default();
    api.patch_status(name, &pp, &Patch::Merge(&patch))
        .await
        .map_err(|e| AppError::Kubernetes(format!("Promote {}/{}: {e}", ns, name)))?;

    let action = if full { "promote_full" } else { "promote_step" };
    tracing::info!(
        "Promoted rollout {}/{} (full={}) by user {}",
        ns,
        name,
        full,
        auth_user.user_id
    );

    record_event(
        pool,
        cluster_id,
        ns,
        name,
        action,
        serde_json::json!({"full": full}),
        Some(auth_user.user_id),
        auth_user.tenant_id,
    )
    .await;

    Ok(serde_json::json!({
        "status": "ok",
        "action": action,
        "rollout": format!("{}/{}", ns, name),
    }))
}

// ─── Rollback ───────────────────────────────────────────────────────────────

/// Abort and rollback a Rollout.
pub async fn rollback(
    pool: &PgPool,
    auth_user: &AuthUser,
    cluster_id: Uuid,
    ns: &str,
    name: &str,
) -> AppResult<serde_json::Value> {
    let cluster = load_and_authorize_cluster(pool, cluster_id, auth_user).await?;

    check_write_permission(pool, auth_user, &cluster).await?;

    let client = build_k8s_client(pool, &cluster).await?;
    let ar = rollout_api_resource();
    let api: Api<DynamicObject> = Api::namespaced_with(client, ns, &ar);

    let patch = serde_json::json!({
        "status": {
            "abort": true
        }
    });

    let pp = PatchParams::default();
    api.patch_status(name, &pp, &Patch::Merge(&patch))
        .await
        .map_err(|e| AppError::Kubernetes(format!("Rollback {}/{}: {e}", ns, name)))?;

    tracing::info!("Rolled back rollout {}/{} by user {}", ns, name, auth_user.user_id);

    record_event(
        pool,
        cluster_id,
        ns,
        name,
        "rollback",
        serde_json::json!({}),
        Some(auth_user.user_id),
        auth_user.tenant_id,
    )
    .await;

    Ok(serde_json::json!({
        "status": "ok",
        "action": "rollback",
        "rollout": format!("{}/{}", ns, name),
    }))
}

// ─── Change Strategy ────────────────────────────────────────────────────────

/// Change the deployment strategy of an Argo Rollout.
pub async fn change_strategy(
    pool: &PgPool,
    auth_user: &AuthUser,
    cluster_id: Uuid,
    ns: &str,
    name: &str,
    req: ChangeStrategyRequest,
) -> AppResult<serde_json::Value> {
    let cluster = load_and_authorize_cluster(pool, cluster_id, auth_user).await?;
    check_write_permission(pool, auth_user, &cluster).await?;
    let client = build_k8s_client(pool, &cluster).await?;

    let ar = rollout_api_resource();
    let api: Api<DynamicObject> = Api::namespaced_with(client.clone(), ns, &ar);

    // Build the strategy patch based on requested type
    let strategy_patch = match req.strategy.as_str() {
        "canary" => {
            let steps = req.canary_steps.as_ref().map(|steps| {
                steps
                    .iter()
                    .map(|s| {
                        if let Some(w) = s.set_weight {
                            serde_json::json!({ "setWeight": w })
                        } else if let Some(ref p) = s.pause {
                            serde_json::json!({ "pause": p })
                        } else {
                            serde_json::json!({})
                        }
                    })
                    .collect::<Vec<_>>()
            });

            let mut canary = serde_json::json!({});
            if let Some(steps) = steps {
                canary["steps"] = serde_json::json!(steps);
            }

            serde_json::json!({
                "spec": {
                    "strategy": {
                        "canary": canary,
                        "blueGreen": null
                    }
                }
            })
        }
        "blueGreen" => {
            let active_svc = req
                .active_service
                .as_deref()
                .ok_or_else(|| AppError::BadRequest("activeService is required for blueGreen strategy".into()))?;
            let preview_svc = req
                .preview_service
                .as_deref()
                .ok_or_else(|| AppError::BadRequest("previewService is required for blueGreen strategy".into()))?;

            // Ensure the preview Service exists (create if missing)
            ensure_preview_service(&client, ns, active_svc, preview_svc).await?;

            let auto_promo = req.auto_promotion_enabled.unwrap_or(false);

            serde_json::json!({
                "spec": {
                    "strategy": {
                        "blueGreen": {
                            "activeService": active_svc,
                            "previewService": preview_svc,
                            "autoPromotionEnabled": auto_promo
                        },
                        "canary": null
                    }
                }
            })
        }
        "rollingUpdate" => {
            // Rolling update = canary with no steps (immediate full rollout)
            serde_json::json!({
                "spec": {
                    "strategy": {
                        "canary": {},
                        "blueGreen": null
                    }
                }
            })
        }
        other => {
            return Err(AppError::BadRequest(format!(
                "Unknown strategy '{}'. Supported: canary, blueGreen, rollingUpdate",
                other
            )));
        }
    };

    let pp = PatchParams::default();
    api.patch(name, &pp, &Patch::Merge(&strategy_patch))
        .await
        .map_err(|e| AppError::Kubernetes(format!("Change strategy {}/{}: {e}", ns, name)))?;

    tracing::info!(
        "Changed strategy of {}/{} to '{}' by user {}",
        ns,
        name,
        req.strategy,
        auth_user.user_id
    );

    record_event(
        pool,
        cluster_id,
        ns,
        name,
        "change_strategy",
        serde_json::json!({"strategy": req.strategy}),
        Some(auth_user.user_id),
        auth_user.tenant_id,
    )
    .await;

    Ok(serde_json::json!({
        "status": "ok",
        "action": "change_strategy",
        "strategy": req.strategy,
        "rollout": format!("{}/{}", ns, name),
    }))
}

/// Ensure a preview Service exists for blueGreen strategy.
/// If it doesn't exist, clone it from the active Service.
async fn ensure_preview_service(
    client: &kube::Client,
    namespace: &str,
    active_svc_name: &str,
    preview_svc_name: &str,
) -> AppResult<()> {
    use k8s_openapi::api::core::v1::Service;
    use kube::api::PostParams;

    let svc_api: Api<Service> = Api::namespaced(client.clone(), namespace);

    // Check if preview service already exists
    match svc_api.get(preview_svc_name).await {
        Ok(_) => return Ok(()), // Already exists
        Err(kube::Error::Api(ref err)) if err.code == 404 => {
            // Not found — create it
        }
        Err(e) => {
            return Err(AppError::Kubernetes(format!(
                "Check preview service {}: {e}",
                preview_svc_name
            )));
        }
    }

    // Get the active service to clone its spec
    let active = svc_api
        .get(active_svc_name)
        .await
        .map_err(|e| AppError::Kubernetes(format!("Cannot find active service '{}': {e}", active_svc_name)))?;

    let active_spec = active
        .spec
        .ok_or_else(|| AppError::Internal("Active service has no spec".into()))?;

    // Build the preview service (clone spec, new name, remove clusterIP)
    let preview = Service {
        metadata: kube::api::ObjectMeta {
            name: Some(preview_svc_name.to_string()),
            namespace: Some(namespace.to_string()),
            labels: active.metadata.labels.clone(),
            ..Default::default()
        },
        spec: Some(k8s_openapi::api::core::v1::ServiceSpec {
            selector: active_spec.selector.clone(),
            ports: active_spec.ports.clone(),
            type_: active_spec.type_.clone(),
            // Do NOT copy clusterIP — let K8s assign a new one
            ..Default::default()
        }),
        ..Default::default()
    };

    svc_api
        .create(&PostParams::default(), &preview)
        .await
        .map_err(|e| AppError::Kubernetes(format!("Create preview service '{}': {e}", preview_svc_name)))?;

    tracing::info!(
        "Created preview service '{}' in namespace '{}' (cloned from '{}')",
        preview_svc_name,
        namespace,
        active_svc_name
    );

    Ok(())
}

// ─── List Events ────────────────────────────────────────────────────────────

/// List deployment events (audit log). Optional filters: cluster_id, namespace, rollout_name.
pub async fn list_events(
    pool: &PgPool,
    auth_user: &AuthUser,
    cluster_id: Option<Uuid>,
    namespace: Option<&str>,
    rollout_name: Option<&str>,
) -> AppResult<Vec<DeploymentEvent>> {
    let rows = if let (Some(cid), Some(ns), Some(name)) = (cluster_id, namespace, rollout_name) {
        sqlx::query_as::<_, DeploymentEvent>(
            r#"SELECT * FROM deployment_events
               WHERE cluster_id = $1 AND namespace = $2 AND rollout_name = $3
               AND ($4::UUID IS NULL OR tenant_id = $4)
               ORDER BY created_at DESC LIMIT 100"#,
        )
        .bind(cid)
        .bind(ns)
        .bind(name)
        .bind(auth_user.tenant_id)
        .fetch_all(pool)
        .await?
    } else if let Some(cid) = cluster_id {
        sqlx::query_as::<_, DeploymentEvent>(
            r#"SELECT * FROM deployment_events
               WHERE cluster_id = $1
               AND ($2::UUID IS NULL OR tenant_id = $2)
               ORDER BY created_at DESC LIMIT 100"#,
        )
        .bind(cid)
        .bind(auth_user.tenant_id)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as::<_, DeploymentEvent>(
            r#"SELECT * FROM deployment_events
               WHERE ($1::UUID IS NULL OR tenant_id = $1)
               ORDER BY created_at DESC LIMIT 100"#,
        )
        .bind(auth_user.tenant_id)
        .fetch_all(pool)
        .await?
    };

    Ok(rows)
}

// ─── Record Event ───────────────────────────────────────────────────────────

/// Fire-and-forget: record a deployment event to DB.
/// `user_id` / `tenant_id` are optional — automated events (ArgoCD sync, rollout watcher) have no user.
#[allow(clippy::too_many_arguments)]
pub async fn record_event(
    pool: &PgPool,
    cluster_id: Uuid,
    namespace: &str,
    rollout_name: &str,
    action: &str,
    detail: serde_json::Value,
    user_id: Option<Uuid>,
    tenant_id: Option<Uuid>,
) {
    if let Err(e) = sqlx::query(
        r#"INSERT INTO deployment_events (cluster_id, namespace, rollout_name, action, detail, user_id, tenant_id)
           VALUES ($1, $2, $3, $4, $5, $6, $7)"#,
    )
    .bind(cluster_id)
    .bind(namespace)
    .bind(rollout_name)
    .bind(action)
    .bind(&detail)
    .bind(user_id)
    .bind(tenant_id)
    .execute(pool)
    .await
    {
        tracing::error!("Failed to record deployment event: {}", e);
    }
}

// ─── Permission Check ───────────────────────────────────────────────────────

/// Check that the user has write access to the cluster's associated account.
pub async fn check_write_permission(
    pool: &PgPool,
    auth_user: &AuthUser,
    cluster: &crate::models::cluster::Cluster,
) -> AppResult<()> {
    if auth_user.is_super_admin() {
        return Ok(());
    }

    if let Some(ref account_id) = cluster.account_id {
        // Look up the internal account UUID
        let maybe_id: Option<Uuid> =
            sqlx::query_scalar("SELECT id FROM cloud_accounts WHERE account_id = $1 LIMIT 1")
                .bind(account_id)
                .fetch_optional(pool)
                .await
                .map_err(|e| AppError::Internal(format!("DB error: {e}")))?;

        if let Some(internal_id) = maybe_id {
            let can_write: bool = sqlx::query_scalar(
                r#"SELECT EXISTS(
                    SELECT 1 FROM user_account_access
                    WHERE user_id = $1 AND account_id = $2 AND role = 'admin'
                )"#,
            )
            .bind(auth_user.user_id)
            .bind(internal_id)
            .fetch_one(pool)
            .await
            .unwrap_or(false);

            if !can_write {
                return Err(AppError::Forbidden(
                    "Read-only access: cannot modify rollouts".to_string(),
                ));
            }
        }
    }

    Ok(())
}
