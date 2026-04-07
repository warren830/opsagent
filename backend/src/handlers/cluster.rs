use axum::{
    Json,
    extract::{Path, State},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::AppState;
use crate::error::{AppError, AppResult};
use crate::middleware::auth::AuthUser;
use crate::models::cloud_account::CloudAccount;
use crate::models::cluster::{Cluster, CreateClusterRequest, UpdateClusterRequest};

/// GET /api/clusters
pub async fn list(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
) -> AppResult<Json<Vec<Cluster>>> {
    let rows = if auth_user.is_super_admin() {
        sqlx::query_as::<_, Cluster>("SELECT * FROM clusters ORDER BY name")
            .fetch_all(&state.pool)
            .await?
    } else {
        sqlx::query_as::<_, Cluster>("SELECT * FROM clusters WHERE tenant_id = $1 ORDER BY name")
            .bind(auth_user.tenant_id)
            .fetch_all(&state.pool)
            .await?
    };
    Ok(Json(rows))
}

/// POST /api/clusters
pub async fn create(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Json(req): Json<CreateClusterRequest>,
) -> AppResult<Json<Cluster>> {
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
    .fetch_one(&state.pool)
    .await?;

    Ok(Json(row))
}

/// PUT /api/clusters/:id
pub async fn update(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateClusterRequest>,
) -> AppResult<Json<Cluster>> {
    if !auth_user.is_super_admin() {
        let existing = sqlx::query_as::<_, Cluster>("SELECT * FROM clusters WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.pool)
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
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Cluster not found".to_string()))?;

    Ok(Json(row))
}

/// DELETE /api/clusters/:id
pub async fn delete(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    if !auth_user.is_super_admin() {
        let existing = sqlx::query_as::<_, Cluster>("SELECT * FROM clusters WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Cluster not found".to_string()))?;
        if existing.tenant_id != auth_user.tenant_id {
            return Err(AppError::Forbidden("Access denied".to_string()));
        }
    }

    let result = sqlx::query("DELETE FROM clusters WHERE id = $1")
        .bind(id)
        .execute(&state.pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Cluster not found".to_string()));
    }

    Ok(Json(serde_json::json!({"message": "Cluster deleted"})))
}

// ─── EKS Cluster Discovery ─────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct EksListOutput {
    clusters: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DiscoverResult {
    pub discovered: usize,
    pub errors: Vec<String>,
}

/// Common AWS regions to scan when an account has "All Regions" (empty regions array)
const DEFAULT_SCAN_REGIONS: &[&str] = &[
    "us-east-1",
    "us-east-2",
    "us-west-1",
    "us-west-2",
    "eu-west-1",
    "eu-west-2",
    "eu-central-1",
    "ap-northeast-1",
    "ap-southeast-1",
    "ap-southeast-2",
    "ap-south-1",
];

/// Build env vars for AWS CLI: for accounts with profile use --profile,
/// for accounts with role_arn assume-role using the root profile first.
async fn build_account_env(
    account: &CloudAccount,
    root_profile: &Option<String>,
) -> Result<Vec<(String, String)>, String> {
    // Account has its own profile — use it directly
    if let Some(ref profile) = account.profile {
        return Ok(vec![("AWS_PROFILE".to_string(), profile.clone())]);
    }

    // Account has role_arn — assume-role using root profile
    if let Some(ref role_arn) = account.role_arn {
        let mut cmd = tokio::process::Command::new("aws");
        cmd.args([
            "sts",
            "assume-role",
            "--role-arn",
            role_arn,
            "--role-session-name",
            "openops-discover",
            "--duration-seconds",
            "900",
            "--output",
            "json",
        ]);
        if let Some(profile) = root_profile {
            cmd.args(["--profile", profile]);
        }
        let output = cmd.output().await.map_err(|e| format!("aws CLI error: {e}"))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("assume-role failed: {}", stderr.trim()));
        }
        let body: serde_json::Value =
            serde_json::from_slice(&output.stdout).map_err(|e| format!("parse error: {e}"))?;
        let creds = body.pointer("/Credentials").ok_or("No Credentials")?;
        let ak = creds
            .get("AccessKeyId")
            .and_then(|v| v.as_str())
            .ok_or("Missing AccessKeyId")?;
        let sk = creds
            .get("SecretAccessKey")
            .and_then(|v| v.as_str())
            .ok_or("Missing SecretAccessKey")?;
        let st = creds
            .get("SessionToken")
            .and_then(|v| v.as_str())
            .ok_or("Missing SessionToken")?;
        return Ok(vec![
            ("AWS_ACCESS_KEY_ID".to_string(), ak.to_string()),
            ("AWS_SECRET_ACCESS_KEY".to_string(), sk.to_string()),
            ("AWS_SESSION_TOKEN".to_string(), st.to_string()),
        ]);
    }

    // No credentials at all — fall back to root profile if available
    match root_profile {
        Some(p) => Ok(vec![("AWS_PROFILE".to_string(), p.clone())]),
        None => Ok(vec![]),
    }
}

/// POST /api/clusters/discover
/// Discover EKS clusters by iterating each AWS account with its own credentials.
pub async fn discover(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
) -> AppResult<Json<DiscoverResult>> {
    let tenant_id = auth_user.tenant_id;

    let accounts = if auth_user.is_super_admin() {
        sqlx::query_as::<_, CloudAccount>("SELECT * FROM cloud_accounts WHERE provider = 'aws' AND is_mock = false")
            .fetch_all(&state.pool)
            .await?
    } else {
        sqlx::query_as::<_, CloudAccount>(
            "SELECT * FROM cloud_accounts WHERE provider = 'aws' AND is_mock = false AND tenant_id IS NOT DISTINCT FROM $1",
        )
        .bind(tenant_id)
        .fetch_all(&state.pool)
        .await?
    };

    // Find the root account's profile (the one with a local profile configured)
    let root_profile: Option<String> = accounts.iter().find_map(|a| a.profile.clone());

    let mut total_discovered: usize = 0;
    let mut errors: Vec<String> = Vec::new();

    for account in &accounts {
        // Build credentials for this account
        let env_vars = match build_account_env(account, &root_profile).await {
            Ok(env) => env,
            Err(e) => {
                errors.push(format!("{}: {}", account.name, e));
                continue;
            }
        };

        // Empty regions = All Regions → scan default list
        let regions: Vec<String> = if account.regions.is_empty() {
            DEFAULT_SCAN_REGIONS.iter().map(|s| s.to_string()).collect()
        } else {
            account.regions.clone()
        };

        for region in &regions {
            // aws eks list-clusters
            let mut cmd = tokio::process::Command::new("aws");
            cmd.args(["eks", "list-clusters", "--region", region, "--output", "json"]);
            for (k, v) in &env_vars {
                cmd.env(k, v);
            }

            let output = match cmd.output().await {
                Ok(o) => o,
                Err(e) => {
                    errors.push(format!("{}/{}: {}", account.name, region, e));
                    continue;
                }
            };

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                if !stderr.contains("AccessDeniedException") && !stderr.contains("is not authorized") {
                    errors.push(format!("{}/{}: {}", account.name, region, stderr.trim()));
                }
                continue;
            }

            let list_output: EksListOutput = match serde_json::from_slice(&output.stdout) {
                Ok(o) => o,
                Err(e) => {
                    errors.push(format!("{}/{}: parse: {}", account.name, region, e));
                    continue;
                }
            };

            for cluster_name in &list_output.clusters {
                // aws eks describe-cluster
                let mut desc_cmd = tokio::process::Command::new("aws");
                desc_cmd.args([
                    "eks",
                    "describe-cluster",
                    "--name",
                    cluster_name,
                    "--region",
                    region,
                    "--output",
                    "json",
                ]);
                for (k, v) in &env_vars {
                    desc_cmd.env(k, v);
                }

                let desc_output = match desc_cmd.output().await {
                    Ok(o) => o,
                    Err(e) => {
                        errors.push(format!("describe {} error: {}", cluster_name, e));
                        continue;
                    }
                };

                let mut cluster_status = "unknown".to_string();
                let mut config = serde_json::json!({});

                if desc_output.status.success()
                    && let Ok(body) = serde_json::from_slice::<serde_json::Value>(&desc_output.stdout)
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
                    });
                }

                let upsert_result = sqlx::query_as::<_, Cluster>(
                    r#"INSERT INTO clusters (name, cloud, cluster_type, account_id, region, status, is_discovered, last_seen_at, config, tenant_id)
                       VALUES ($1, 'aws', 'eks', $2, $3, $4, true, NOW(), $5, $6)
                       ON CONFLICT (tenant_id, name)
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
                .bind(&account.account_id)
                .bind(region)
                .bind(&cluster_status)
                .bind(&config)
                .bind(tenant_id)
                .fetch_optional(&state.pool)
                .await;

                match upsert_result {
                    Ok(Some(_)) => total_discovered += 1,
                    Ok(None) => {}
                    Err(e) => {
                        tracing::warn!("Failed to upsert cluster {}: {}", cluster_name, e);
                        errors.push(format!("upsert {}: {}", cluster_name, e));
                    }
                }
            }
        }
    }

    Ok(Json(DiscoverResult {
        discovered: total_discovered,
        errors,
    }))
}
