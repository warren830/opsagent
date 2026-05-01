use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::middleware::auth::AuthUser;
use crate::models::cloud_account::CloudAccount;
use crate::models::cluster::{Cluster, CreateClusterRequest, UpdateClusterRequest};
use crate::services::k8s::build_account_env;

// ─── Discovery types ──────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct EksListOutput {
    clusters: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DiscoverResult {
    pub discovered: usize,
    pub errors: Vec<String>,
}

// ─── CRUD ─────────────────────────────────────────────────────────────────

/// List clusters visible to the authenticated user.
pub async fn list(pool: &PgPool, auth_user: &AuthUser) -> AppResult<Vec<Cluster>> {
    let rows = if auth_user.is_super_admin() {
        sqlx::query_as::<_, Cluster>("SELECT * FROM clusters ORDER BY name")
            .fetch_all(pool)
            .await?
    } else {
        sqlx::query_as::<_, Cluster>("SELECT * FROM clusters WHERE tenant_id = $1 ORDER BY name")
            .bind(auth_user.tenant_id)
            .fetch_all(pool)
            .await?
    };
    Ok(rows)
}

/// Create a new cluster record.
pub async fn create(
    pool: &PgPool,
    auth_user: &AuthUser,
    req: CreateClusterRequest,
) -> AppResult<Cluster> {
    if req.name.trim().is_empty() {
        return Err(AppError::BadRequest("Name is required".to_string()));
    }

    let row = sqlx::query_as::<_, Cluster>(
        r#"INSERT INTO clusters (name, cloud, cluster_type, account_id, region, role_name, description, config, tenant_id)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
           RETURNING *"#,
    )
    .bind(&req.name)
    .bind(&req.cloud)
    .bind(&req.cluster_type)
    .bind(&req.account_id)
    .bind(&req.region)
    .bind(&req.role_name)
    .bind(&req.description)
    .bind(&req.config)
    .bind(auth_user.tenant_id)
    .fetch_one(pool)
    .await?;

    Ok(row)
}

/// Update an existing cluster (ownership check for non-super-admins).
pub async fn update(
    pool: &PgPool,
    auth_user: &AuthUser,
    id: Uuid,
    req: UpdateClusterRequest,
) -> AppResult<Cluster> {
    if !auth_user.is_super_admin() {
        let existing = sqlx::query_as::<_, Cluster>("SELECT * FROM clusters WHERE id = $1")
            .bind(id)
            .fetch_optional(pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Cluster not found".to_string()))?;
        if existing.tenant_id != auth_user.tenant_id {
            return Err(AppError::Forbidden("Access denied".to_string()));
        }
    }

    let row = sqlx::query_as::<_, Cluster>(
        r#"UPDATE clusters SET
           name = COALESCE($2, name),
           cloud = COALESCE($3, cloud),
           cluster_type = COALESCE($4, cluster_type),
           account_id = COALESCE($5, account_id),
           region = COALESCE($6, region),
           role_name = COALESCE($7, role_name),
           description = COALESCE($8, description),
           status = COALESCE($9, status),
           config = COALESCE($10, config),
           updated_at = NOW()
           WHERE id = $1
           RETURNING *"#,
    )
    .bind(id)
    .bind(&req.name)
    .bind(&req.cloud)
    .bind(&req.cluster_type)
    .bind(&req.account_id)
    .bind(&req.region)
    .bind(&req.role_name)
    .bind(&req.description)
    .bind(&req.status)
    .bind(&req.config)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Cluster not found".to_string()))?;

    Ok(row)
}

/// Delete a cluster (ownership check for non-super-admins).
pub async fn delete(pool: &PgPool, auth_user: &AuthUser, id: Uuid) -> AppResult<()> {
    if !auth_user.is_super_admin() {
        let existing = sqlx::query_as::<_, Cluster>("SELECT * FROM clusters WHERE id = $1")
            .bind(id)
            .fetch_optional(pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Cluster not found".to_string()))?;
        if existing.tenant_id != auth_user.tenant_id {
            return Err(AppError::Forbidden("Access denied".to_string()));
        }
    }

    let result = sqlx::query("DELETE FROM clusters WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Cluster not found".to_string()));
    }

    Ok(())
}

// ─── EKS Cluster Discovery ───────────────────────────────────────────────

/// Dynamically discover all enabled regions for an AWS account via `aws ec2 describe-regions`.
/// Falls back to us-east-1 only if the API call fails.
async fn discover_enabled_regions(env_vars: &[(String, String)]) -> Vec<String> {
    #[derive(Debug, serde::Deserialize)]
    struct RegionInfo {
        #[serde(rename = "RegionName")]
        region_name: String,
    }
    #[derive(Debug, serde::Deserialize)]
    struct DescribeRegionsOutput {
        #[serde(rename = "Regions")]
        regions: Vec<RegionInfo>,
    }

    let mut cmd = tokio::process::Command::new("aws");
    cmd.args([
        "ec2",
        "describe-regions",
        "--filters",
        "Name=opt-in-status,Values=opt-in-not-required,opted-in",
        "--query",
        "Regions[].RegionName",
        "--output",
        "json",
    ]);
    for (k, v) in env_vars {
        cmd.env(k, v);
    }

    match cmd.output().await {
        Ok(output) if output.status.success() => {
            // --query returns a flat JSON array of strings
            if let Ok(regions) = serde_json::from_slice::<Vec<String>>(&output.stdout)
                && !regions.is_empty()
            {
                return regions;
            }
            // Fallback: parse full response (without --query)
            if let Ok(resp) = serde_json::from_slice::<DescribeRegionsOutput>(&output.stdout) {
                return resp.regions.into_iter().map(|r| r.region_name).collect();
            }
            vec!["us-east-1".to_string()]
        }
        _ => vec!["us-east-1".to_string()],
    }
}

/// Core cluster discovery logic — scans all AWS accounts concurrently.
/// Called by both the HTTP handler and the background scheduler.
/// When `tenant_id` is None, scans all accounts (super_admin / scheduler mode).
pub async fn discover_all_clusters(
    pool: &PgPool,
    tenant_id: Option<Uuid>,
) -> AppResult<DiscoverResult> {
    use futures::stream::{self, StreamExt};
    use std::sync::Arc;
    use tokio::sync::Mutex;

    let accounts = match tenant_id {
        Some(tid) => {
            sqlx::query_as::<_, CloudAccount>(
                "SELECT * FROM cloud_accounts WHERE provider = 'aws' AND is_mock = false AND tenant_id IS NOT DISTINCT FROM $1",
            )
            .bind(tid)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query_as::<_, CloudAccount>(
                "SELECT * FROM cloud_accounts WHERE provider = 'aws' AND is_mock = false",
            )
            .fetch_all(pool)
            .await?
        }
    };

    let root_profile: Option<String> = accounts.iter().find_map(|a| a.profile.clone());

    // Build (account_name, account_id, tenant_id, env_vars, region) tuples for all scan tasks
    type ScanTask = (String, String, Option<Uuid>, Vec<(String, String)>, String);
    let mut scan_tasks: Vec<ScanTask> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    for account in &accounts {
        let env_vars = match build_account_env(account, &root_profile).await {
            Ok(env) => env,
            Err(e) => {
                errors.push(format!("{}: {}", account.name, e));
                continue;
            }
        };

        let regions: Vec<String> = if account.regions.is_empty() {
            discover_enabled_regions(&env_vars).await
        } else {
            account.regions.clone()
        };

        for region in regions {
            scan_tasks.push((
                account.name.clone(),
                account.account_id.clone().unwrap_or_default(),
                account.tenant_id,
                env_vars.clone(),
                region,
            ));
        }
    }

    let total_discovered = Arc::new(Mutex::new(0usize));
    let shared_errors = Arc::new(Mutex::new(errors));

    // Run all region scans concurrently, capped at 8 to avoid AWS throttling
    stream::iter(scan_tasks)
        .for_each_concurrent(
            8,
            |(account_name, account_id, acct_tenant_id, env_vars, region)| {
                let pool = pool.clone();
                let total = total_discovered.clone();
                let errs = shared_errors.clone();

                async move {
                    let mut cmd = tokio::process::Command::new("aws");
                    cmd.args([
                        "eks",
                        "list-clusters",
                        "--region",
                        &region,
                        "--output",
                        "json",
                    ]);
                    for (k, v) in &env_vars {
                        cmd.env(k, v);
                    }

                    let output = match cmd.output().await {
                        Ok(o) => o,
                        Err(e) => {
                            errs.lock()
                                .await
                                .push(format!("{}/{}: {}", account_name, region, e));
                            return;
                        }
                    };

                    if !output.status.success() {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        if !stderr.contains("AccessDeniedException")
                            && !stderr.contains("is not authorized")
                        {
                            errs.lock()
                                .await
                                .push(format!("{}/{}: {}", account_name, region, stderr.trim()));
                        }
                        return;
                    }

                    let list_output: EksListOutput = match serde_json::from_slice(&output.stdout) {
                        Ok(o) => o,
                        Err(e) => {
                            errs.lock()
                                .await
                                .push(format!("{}/{}: parse: {}", account_name, region, e));
                            return;
                        }
                    };

                    for cluster_name in &list_output.clusters {
                        let mut desc_cmd = tokio::process::Command::new("aws");
                        desc_cmd.args([
                            "eks",
                            "describe-cluster",
                            "--name",
                            cluster_name,
                            "--region",
                            &region,
                            "--output",
                            "json",
                        ]);
                        for (k, v) in &env_vars {
                            desc_cmd.env(k, v);
                        }

                        let desc_output = match desc_cmd.output().await {
                            Ok(o) => o,
                            Err(e) => {
                                errs.lock()
                                    .await
                                    .push(format!("describe {} error: {}", cluster_name, e));
                                continue;
                            }
                        };

                        let mut cluster_status = "unknown".to_string();
                        let mut config = serde_json::json!({});

                        if desc_output.status.success()
                            && let Ok(body) =
                                serde_json::from_slice::<serde_json::Value>(&desc_output.stdout)
                            && let Some(cluster) = body.get("cluster")
                        {
                            cluster_status = cluster
                                .get("status")
                                .and_then(|v| v.as_str())
                                .unwrap_or("unknown")
                                .to_string();
                            config = serde_json::json!({
                                "version": cluster.get("version").and_then(|v| v.as_str()),
                                "endpoint": cluster.get("endpoint").and_then(|v| v.as_str()),
                                "platform_version": cluster.get("platformVersion").and_then(|v| v.as_str()),
                                "arn": cluster.get("arn").and_then(|v| v.as_str()),
                                "certificate_authority": cluster.pointer("/certificateAuthority/data")
                                    .and_then(|v| v.as_str()),
                            });
                        }

                        let upsert_result = sqlx::query_as::<_, Cluster>(
                            r#"INSERT INTO clusters (name, cloud, cluster_type, account_id, region, status, is_discovered, last_seen_at, config, tenant_id)
                           VALUES ($1, 'aws', 'eks', $2, $3, $4, true, NOW(), $5, $6)
                           ON CONFLICT (COALESCE(tenant_id, '00000000-0000-0000-0000-000000000000'::uuid), name)
                           DO UPDATE SET
                             status = EXCLUDED.status,
                             region = EXCLUDED.region,
                             account_id = EXCLUDED.account_id,
                             config = EXCLUDED.config,
                             is_discovered = true,
                             last_seen_at = NOW(),
                             updated_at = NOW()
                           RETURNING *"#,
                        )
                        .bind(cluster_name)
                        .bind(&account_id)
                        .bind(&region)
                        .bind(&cluster_status)
                        .bind(&config)
                        .bind(acct_tenant_id)
                        .fetch_optional(&pool)
                        .await;

                        match upsert_result {
                            Ok(Some(_)) => *total.lock().await += 1,
                            Ok(None) => {}
                            Err(e) => {
                                tracing::warn!(
                                    "Failed to upsert cluster {}: {}",
                                    cluster_name,
                                    e
                                );
                                errs.lock()
                                    .await
                                    .push(format!("upsert {}: {}", cluster_name, e));
                            }
                        }
                    }
                }
            },
        )
        .await;

    let discovered = *total_discovered.lock().await;
    let final_errors = shared_errors.lock().await.clone();

    // Reconcile pass: any `is_discovered=true` cluster we didn't touch during
    // this run (last_seen_at older than 1 hour — grace window for transient
    // AWS API flakes) is very likely gone from AWS. Mark `stale` instead of
    // deleting so history survives and the UI can show an "offline" badge;
    // the user confirms before a hard-delete.
    let reconcile_result = match tenant_id {
        Some(tid) => sqlx::query(
            r#"UPDATE clusters
               SET status = 'stale', updated_at = NOW()
               WHERE is_discovered = true
                 AND tenant_id IS NOT DISTINCT FROM $1
                 AND (last_seen_at IS NULL OR last_seen_at < NOW() - INTERVAL '1 hour')
                 AND status != 'stale'"#,
        )
        .bind(tid)
        .execute(pool)
        .await,
        None => sqlx::query(
            r#"UPDATE clusters
               SET status = 'stale', updated_at = NOW()
               WHERE is_discovered = true
                 AND (last_seen_at IS NULL OR last_seen_at < NOW() - INTERVAL '1 hour')
                 AND status != 'stale'"#,
        )
        .execute(pool)
        .await,
    };
    if let Ok(res) = reconcile_result
        && res.rows_affected() > 0
    {
        tracing::info!("Reconciled {} stale clusters", res.rows_affected());
    }

    Ok(DiscoverResult {
        discovered,
        errors: final_errors,
    })
}
