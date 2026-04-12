use axum::{
    Json,
    extract::{Path, Query, State},
};
use uuid::Uuid;

use crate::AppState;
use crate::error::{AppError, AppResult};
use crate::middleware::auth::AuthUser;
use crate::models::cloud_account::CloudAccount;
use crate::models::finding::{
    CategoryCounts, DashboardStats, Finding, FindingListQuery, Scan, ScanRequest, ServiceCount, SeverityCounts,
};

// ─── Screener paths ────────────────────────────────────────────────────────

const SCREENER_GLOBAL_REPO: &str = "https://github.com/aws-samples/service-screener-v2.git";
const SCREENER_CHINA_REPO: &str = "https://github.com/lijh-aws-tools/service-screener-cn.git";

/// Resolve screener base directory.
/// 1. SCREENER_DIR env var (explicit override)
/// 2. PROJECT_ROOT/.ops/screener (local dev + Docker COPY)
/// 3. fallback: cwd/.ops/screener
fn screener_dir() -> std::path::PathBuf {
    if let Ok(dir) = std::env::var("SCREENER_DIR") {
        return std::path::PathBuf::from(dir);
    }
    // In Docker: WORKDIR is /app, .ops/ is copied alongside binary
    // Local dev: run from project root or backend/
    let candidates = [
        std::path::PathBuf::from(".ops/screener"),    // cwd = project root
        std::path::PathBuf::from("../.ops/screener"), // cwd = backend/
    ];
    for c in &candidates {
        if c.exists() {
            return c.clone();
        }
    }
    // Default: project root relative
    std::path::PathBuf::from(".ops/screener")
}

fn is_china_region(region: &str) -> bool {
    region.starts_with("cn-")
}

/// Default region when none specified — scan a single region to keep it fast
const DEFAULT_SCAN_REGIONS: &[&str] = &["us-east-1"];

// ─── GET /api/resources/screener/status ─────────────────────────────────────

/// Check if screener is installed
pub async fn screener_status(_auth_user: axum::Extension<AuthUser>) -> AppResult<Json<serde_json::Value>> {
    let base = screener_dir();
    let global_ok = base.join("global/main.py").exists();
    let china_ok = base.join("china/main.py").exists();

    // Read supported services from info.json (separate per variant)
    let mut global_services: Vec<String> = Vec::new();
    let mut china_services: Vec<String> = Vec::new();

    for (variant, list) in [("global", &mut global_services), ("china", &mut china_services)] {
        let info_path = base.join(variant).join("info.json");
        if let Ok(content) = std::fs::read_to_string(&info_path)
            && let Ok(data) = serde_json::from_str::<serde_json::Value>(&content)
            && let Some(obj) = data.as_object()
        {
            for key in obj.keys() {
                list.push(key.clone());
            }
        }
        list.sort();
    }

    Ok(Json(serde_json::json!({
        "installed": global_ok || china_ok,
        "global": global_ok,
        "china": china_ok,
        "path": base.to_string_lossy(),
        "globalServices": global_services,
        "chinaServices": china_services,
    })))
}

// ─── POST /api/resources/screener/setup ────────────────────────────────────

/// Clone and install service-screener repos
pub async fn setup_screener(_auth_user: axum::Extension<AuthUser>) -> AppResult<Json<serde_json::Value>> {
    let base = screener_dir();

    let global_dir = base.join("global");
    let china_dir = base.join("china");

    let mut errors: Vec<String> = Vec::new();

    // Clone or pull global
    if global_dir.exists() {
        let output = tokio::process::Command::new("git")
            .args(["pull"])
            .current_dir(&global_dir)
            .output()
            .await
            .map_err(|e| AppError::Internal(format!("git pull failed: {e}")))?;
        if !output.status.success() {
            errors.push(format!(
                "git pull global: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
    } else {
        tokio::fs::create_dir_all(&base)
            .await
            .map_err(|e| AppError::Internal(format!("mkdir failed: {e}")))?;
        let output = tokio::process::Command::new("git")
            .args(["clone", "--depth", "1", SCREENER_GLOBAL_REPO, "global"])
            .current_dir(&base)
            .output()
            .await
            .map_err(|e| AppError::Internal(format!("git clone failed: {e}")))?;
        if !output.status.success() {
            errors.push(format!(
                "git clone global: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
    }

    // Clone or pull china
    if china_dir.exists() {
        let output = tokio::process::Command::new("git")
            .args(["pull"])
            .current_dir(&china_dir)
            .output()
            .await
            .map_err(|e| AppError::Internal(format!("git pull failed: {e}")))?;
        if !output.status.success() {
            errors.push(format!(
                "git pull china: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
    } else {
        let output = tokio::process::Command::new("git")
            .args(["clone", "--depth", "1", SCREENER_CHINA_REPO, "china"])
            .current_dir(&base)
            .output()
            .await
            .map_err(|e| AppError::Internal(format!("git clone failed: {e}")))?;
        if !output.status.success() {
            errors.push(format!(
                "git clone china: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
    }

    // Install deps via uv venv + uv pip install for both
    for (label, dir) in [("global", &global_dir), ("china", &china_dir)] {
        if dir.exists() {
            // Create venv if not exists
            let venv_dir = dir.join(".venv");
            if !venv_dir.exists() {
                let _ = tokio::process::Command::new("uv")
                    .args(["venv"])
                    .current_dir(dir)
                    .output()
                    .await;
            }

            // Install requirements into venv
            let req_file = dir.join("requirements.txt");
            if req_file.exists() {
                let venv_python = venv_dir.join("bin/python");
                let output = tokio::process::Command::new("uv")
                    .args([
                        "pip",
                        "install",
                        "--python",
                        &venv_python.to_string_lossy(),
                        "-r",
                        "requirements.txt",
                    ])
                    .current_dir(dir)
                    .output()
                    .await
                    .map_err(|e| AppError::Internal(format!("uv pip install failed: {e}")))?;
                if !output.status.success() {
                    errors.push(format!(
                        "uv install {}: {}",
                        label,
                        String::from_utf8_lossy(&output.stderr).trim()
                    ));
                }
            }
        }
    }

    if errors.is_empty() {
        Ok(Json(serde_json::json!({
            "status": "ok",
            "global": global_dir.to_string_lossy(),
            "china": china_dir.to_string_lossy(),
        })))
    } else {
        Ok(Json(serde_json::json!({
            "status": "partial",
            "errors": errors,
            "global": global_dir.to_string_lossy(),
            "china": china_dir.to_string_lossy(),
        })))
    }
}

// ─── POST /api/resources/scan ──────────────────────────────────────────────

/// Build env vars for AWS CLI: profile → assume-role → root profile fallback
async fn build_account_env(
    account: &CloudAccount,
    root_profile: &Option<String>,
) -> Result<Vec<(String, String)>, String> {
    if let Some(ref profile) = account.profile {
        return Ok(vec![("AWS_PROFILE".to_string(), profile.clone())]);
    }

    if let Some(ref role_arn) = account.role_arn {
        let mut cmd = tokio::process::Command::new("aws");
        cmd.args([
            "sts",
            "assume-role",
            "--role-arn",
            role_arn,
            "--role-session-name",
            "ops-screener",
            "--duration-seconds",
            "3600",
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

    match root_profile {
        Some(p) => Ok(vec![("AWS_PROFILE".to_string(), p.clone())]),
        None => Ok(vec![]),
    }
}

/// Trigger a screener scan for an AWS account
pub async fn scan(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Json(req): Json<ScanRequest>,
) -> AppResult<Json<Scan>> {
    let tenant_id = auth_user.tenant_id;

    // Find the account to scan
    let account = if let Some(ref aid) = req.account_id {
        sqlx::query_as::<_, CloudAccount>(
            "SELECT * FROM cloud_accounts WHERE account_id = $1 AND provider = 'aws' AND is_mock = false ORDER BY CASE WHEN source = 'manual' THEN 0 ELSE 1 END LIMIT 1",
        )
        .bind(aid)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Account {} not found", aid)))?
    } else {
        // Use the first available AWS account
        let q = if auth_user.is_super_admin() {
            sqlx::query_as::<_, CloudAccount>(
                "SELECT * FROM cloud_accounts WHERE provider = 'aws' AND is_mock = false LIMIT 1",
            )
            .fetch_optional(&state.pool)
            .await?
        } else {
            sqlx::query_as::<_, CloudAccount>(
                "SELECT * FROM cloud_accounts WHERE provider = 'aws' AND is_mock = false AND tenant_id IS NOT DISTINCT FROM $1 LIMIT 1",
            )
            .bind(tenant_id)
            .fetch_optional(&state.pool)
            .await?
        };
        q.ok_or_else(|| AppError::BadRequest("No AWS account available for scanning".to_string()))?
    };

    // Determine regions
    let regions: Vec<String> = if let Some(ref r) = req.regions {
        r.clone()
    } else if account.regions.is_empty() {
        // Check if any region is china
        DEFAULT_SCAN_REGIONS.iter().map(|s| s.to_string()).collect()
    } else {
        account.regions.clone()
    };

    // Determine services
    let services: Vec<String> = req.services.clone().unwrap_or_default();

    // Create scan record
    let scan_row = sqlx::query_as::<_, Scan>(
        r#"INSERT INTO scans (account_id, account_name, regions, services, status, started_at, tenant_id)
           VALUES ($1, $2, $3, $4, 'running', NOW(), $5)
           RETURNING *"#,
    )
    .bind(&account.account_id)
    .bind(&account.name)
    .bind(&regions)
    .bind(&services)
    .bind(tenant_id)
    .fetch_one(&state.pool)
    .await?;

    let scan_id = scan_row.id;

    // Find root profile for assume-role
    let root_profile: Option<String> = sqlx::query_scalar::<_, String>(
        "SELECT profile FROM cloud_accounts WHERE provider = 'aws' AND profile IS NOT NULL AND tenant_id IS NOT DISTINCT FROM $1 LIMIT 1",
    )
    .bind(tenant_id)
    .fetch_optional(&state.pool)
    .await
    .ok()
    .flatten();

    // Spawn background task
    let pool = state.pool.clone();
    let account_clone = account.clone();
    tokio::spawn(async move {
        let result = run_screener_scan(
            &pool,
            scan_id,
            tenant_id,
            &account_clone,
            &root_profile,
            &regions,
            &services,
        )
        .await;

        if let Err(e) = result {
            tracing::error!("Screener scan {} failed: {}", scan_id, e);
            let _ = sqlx::query(
                "UPDATE scans SET status = 'failed', error_message = $2, completed_at = NOW() WHERE id = $1",
            )
            .bind(scan_id)
            .bind(&e)
            .execute(&pool)
            .await;
        }
    });

    Ok(Json(scan_row))
}

/// Execute the screener and parse results
async fn run_screener_scan(
    pool: &sqlx::PgPool,
    scan_id: Uuid,
    tenant_id: Option<Uuid>,
    account: &CloudAccount,
    root_profile: &Option<String>,
    regions: &[String],
    services: &[String],
) -> Result<(), String> {
    let base = screener_dir();

    // Build credentials
    let env_vars = build_account_env(account, root_profile).await?;

    // Determine if any region is China
    let has_china = regions.iter().any(|r| is_china_region(r));
    let has_global = regions.iter().any(|r| !is_china_region(r));

    let mut total_findings: i64 = 0;
    let mut all_errors: Vec<String> = Vec::new();

    // Run global screener for non-China regions
    if has_global {
        let global_regions: Vec<&str> = regions
            .iter()
            .filter(|r| !is_china_region(r))
            .map(|s| s.as_str())
            .collect();
        let screener_dir = base.join("global");
        match run_single_screener(
            pool,
            scan_id,
            tenant_id,
            account,
            &env_vars,
            &screener_dir,
            &global_regions,
            services,
        )
        .await
        {
            Ok(count) => total_findings += count,
            Err(e) => all_errors.push(format!("global: {}", e)),
        }
    }

    // Run China screener for cn- regions
    if has_china {
        let china_regions: Vec<&str> = regions
            .iter()
            .filter(|r| is_china_region(r))
            .map(|s| s.as_str())
            .collect();
        let screener_dir = base.join("china");
        match run_single_screener(
            pool,
            scan_id,
            tenant_id,
            account,
            &env_vars,
            &screener_dir,
            &china_regions,
            services,
        )
        .await
        {
            Ok(count) => total_findings += count,
            Err(e) => all_errors.push(format!("china: {}", e)),
        }
    }

    // Update scan record
    let error_msg = if all_errors.is_empty() {
        None
    } else {
        Some(all_errors.join("; "))
    };

    let status = if all_errors.is_empty() || total_findings > 0 {
        "completed"
    } else {
        "failed"
    };

    // Build summary aggregation
    let summary = build_scan_summary(pool, scan_id).await;

    // Report path (workspace/scans/{scan_id}/)
    let report_path = format!("workspace/scans/{}", scan_id);

    sqlx::query(
        "UPDATE scans SET status = $2, finding_count = $3, summary = $4, error_message = $5, report_path = $6, completed_at = NOW() WHERE id = $1",
    )
    .bind(scan_id)
    .bind(status)
    .bind(total_findings as i32)
    .bind(&summary)
    .bind(&error_msg)
    .bind(&report_path)
    .execute(pool)
    .await
    .map_err(|e| format!("update scan: {e}"))?;

    Ok(())
}

/// Run screener in a specific directory (global or china) and parse results
#[allow(clippy::too_many_arguments)]
async fn run_single_screener(
    pool: &sqlx::PgPool,
    scan_id: Uuid,
    tenant_id: Option<Uuid>,
    account: &CloudAccount,
    env_vars: &[(String, String)],
    screener_path: &std::path::Path,
    regions: &[&str],
    services: &[String],
) -> Result<i64, String> {
    // Resolve to absolute path to avoid cwd confusion
    let screener_path = screener_path.canonicalize().map_err(|e| {
        format!(
            "Screener not installed at {}. Run POST /api/resources/screener/setup first. ({})",
            screener_path.display(),
            e
        )
    })?;

    let main_py = screener_path.join("main.py");
    if !main_py.exists() {
        return Err(format!("main.py not found in {}", screener_path.display()));
    }

    // Use .venv/bin/python if available, otherwise fallback to python3
    let venv_python = screener_path.join(".venv/bin/python");
    let python_bin = if venv_python.exists() {
        venv_python.to_string_lossy().to_string()
    } else {
        "python3".to_string()
    };
    let mut cmd = tokio::process::Command::new(&python_bin);
    cmd.arg("main.py");
    cmd.args(["--regions", &regions.join(",")]);

    if !services.is_empty() {
        cmd.args(["--services", &services.join(",")]);
    }

    // Pass credentials
    if let Some(ref profile) = account.profile {
        cmd.args(["--profile", profile]);
    }
    for (k, v) in env_vars {
        cmd.env(k, v);
    }

    cmd.current_dir(&screener_path);

    tracing::info!(
        "Running screener: regions={}, services={:?}, dir={}",
        regions.join(","),
        services,
        screener_path.display()
    );

    let output = cmd.output().await.map_err(|e| format!("screener exec error: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::warn!("Screener stderr: {}", stderr);
        // Don't fail if it still produced output
    }

    // Screener writes output to adminlte/aws/{account_id}/api-full.json
    // Also check legacy output/ directory as fallback
    let acct_id = account.account_id.as_deref().unwrap_or("unknown");
    let adminlte_dir = screener_path.join("adminlte/aws").join(acct_id);
    let output_dir = screener_path.join("output");

    let search_paths = [
        adminlte_dir.join("api-full.json"),
        adminlte_dir.join("api-raw.json"),
        output_dir.join("api-full.json"),
        output_dir.join("api-raw.json"),
    ];

    let json_path = search_paths
        .iter()
        .find(|p| p.exists())
        .ok_or_else(|| {
            format!(
                "No output JSON found. Searched:\n  {}\nScreener stderr: {}",
                search_paths
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join("\n  "),
                String::from_utf8_lossy(&output.stderr)
                    .chars()
                    .take(500)
                    .collect::<String>()
            )
        })?
        .clone();

    tracing::info!("Found screener output at: {}", json_path.display());

    // Parse the JSON output
    let content = tokio::fs::read_to_string(&json_path)
        .await
        .map_err(|e| format!("read output: {e}"))?;

    let data: serde_json::Value = serde_json::from_str(&content).map_err(|e| format!("parse output: {e}"))?;

    // Parse and insert findings
    let count = parse_and_insert_findings(pool, scan_id, tenant_id, account, &data).await?;

    // Save report to workspace for agent access & human download
    let workspace_dir = std::path::PathBuf::from("workspace/scans").join(scan_id.to_string());
    let _ = tokio::fs::create_dir_all(&workspace_dir).await;

    // Save api-full.json (agent-readable structured data)
    let _ = tokio::fs::copy(&json_path, workspace_dir.join("api-full.json")).await;

    // Save output.zip (human-readable HTML report) if exists
    let zip_path = screener_path.join("output.zip");
    if zip_path.exists() {
        let _ = tokio::fs::copy(&zip_path, workspace_dir.join("output.zip")).await;
    }

    tracing::info!("Scan {} report saved to {}", scan_id, workspace_dir.display());

    // Clean up scan output only — keep adminlte/aws/res/ (templates) intact
    let _ = tokio::fs::remove_dir_all(adminlte_dir).await; // adminlte/aws/{account_id}
    let _ = tokio::fs::remove_dir_all(&output_dir).await;
    let _ = tokio::fs::remove_file(&zip_path).await;

    Ok(count)
}

/// Parse service screener JSON output and insert findings into DB
async fn parse_and_insert_findings(
    pool: &sqlx::PgPool,
    scan_id: Uuid,
    tenant_id: Option<Uuid>,
    account: &CloudAccount,
    data: &serde_json::Value,
) -> Result<i64, String> {
    let mut count: i64 = 0;

    // The JSON structure: { "service_name": { "summary": { ... }, "detail": { ... } } }
    let obj = data.as_object().ok_or("Expected JSON object at root")?;

    for (service, service_data) in obj {
        // Parse summary section for check definitions
        let summary = match service_data.get("summary") {
            Some(s) => s,
            None => continue,
        };

        let summary_obj = match summary.as_object() {
            Some(o) => o,
            None => continue,
        };

        // Parse detail section for per-resource findings
        let detail = service_data.get("detail");

        // For each check in summary
        for (check_id, check_data) in summary_obj {
            let severity = check_data
                .get("criticality")
                .and_then(|v| v.as_str())
                .unwrap_or("I")
                .to_string();

            let category_main = check_data
                .get("__categoryMain")
                .and_then(|v| v.as_str())
                .unwrap_or("O")
                .to_string();

            let short_desc = check_data
                .get("shortDesc")
                .and_then(|v| v.as_str())
                .unwrap_or(check_id)
                .to_string();

            let description = check_data
                .get("^description")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            // Extract affected resources from summary
            let affected = check_data.get("__affectedResources");

            // Extract links as remediation
            let remediation = check_data.get("__links").and_then(|links| {
                links
                    .as_array()
                    .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>().join("\n"))
            });

            // If we have affected resources, create a finding per resource per region
            if let Some(affected_obj) = affected.and_then(|a| a.as_object()) {
                for (region, resources) in affected_obj {
                    if let Some(resource_arr) = resources.as_array() {
                        for resource in resource_arr {
                            let resource_name = resource.as_str().unwrap_or("unknown").to_string();

                            // Try to get detail for this resource
                            let resource_detail = detail
                                .and_then(|d| d.get(region.as_str()))
                                .and_then(|r| r.get(&resource_name))
                                .and_then(|rd| rd.get(check_id))
                                .cloned()
                                .unwrap_or_else(|| serde_json::json!({}));

                            let result = sqlx::query(
                                r#"INSERT INTO findings (scan_id, service, check_id, severity, category, short_desc, description, resource_name, region, account_id, compliant, remediation, detail, tenant_id)
                                   VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, false, $11, $12, $13)"#,
                            )
                            .bind(scan_id)
                            .bind(service)
                            .bind(check_id)
                            .bind(&severity)
                            .bind(&category_main)
                            .bind(&short_desc)
                            .bind(&description)
                            .bind(&resource_name)
                            .bind(region)
                            .bind(&account.account_id)
                            .bind(&remediation)
                            .bind(&resource_detail)
                            .bind(tenant_id)
                            .execute(pool)
                            .await;

                            match result {
                                Ok(_) => count += 1,
                                Err(e) => {
                                    tracing::warn!("Failed to insert finding {}/{}: {}", service, check_id, e);
                                }
                            }
                        }
                    }
                }
            } else {
                // No affected resources — insert a summary-level finding
                let result = sqlx::query(
                    r#"INSERT INTO findings (scan_id, service, check_id, severity, category, short_desc, description, account_id, compliant, remediation, detail, tenant_id)
                       VALUES ($1, $2, $3, $4, $5, $6, $7, $8, true, $9, $10, $11)"#,
                )
                .bind(scan_id)
                .bind(service)
                .bind(check_id)
                .bind(&severity)
                .bind(&category_main)
                .bind(&short_desc)
                .bind(&description)
                .bind(&account.account_id)
                .bind(&remediation)
                .bind(check_data)
                .bind(tenant_id)
                .execute(pool)
                .await;

                match result {
                    Ok(_) => count += 1,
                    Err(e) => {
                        tracing::warn!("Failed to insert summary finding {}/{}: {}", service, check_id, e);
                    }
                }
            }
        }
    }

    Ok(count)
}

/// Build aggregated summary for a scan
async fn build_scan_summary(pool: &sqlx::PgPool, scan_id: Uuid) -> serde_json::Value {
    // By severity
    let severity: Vec<(String, i64)> = sqlx::query_as(
        "SELECT severity, COUNT(*) as count FROM findings WHERE scan_id = $1 AND compliant = false GROUP BY severity",
    )
    .bind(scan_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    // By category
    let category: Vec<(String, i64)> = sqlx::query_as(
        "SELECT category, COUNT(*) as count FROM findings WHERE scan_id = $1 AND compliant = false GROUP BY category",
    )
    .bind(scan_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    // By service
    let service: Vec<(String, i64)> = sqlx::query_as(
        "SELECT service, COUNT(*) as count FROM findings WHERE scan_id = $1 AND compliant = false GROUP BY service ORDER BY count DESC",
    )
    .bind(scan_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let severity_map: std::collections::HashMap<String, i64> = severity.into_iter().collect();
    let category_map: std::collections::HashMap<String, i64> = category.into_iter().collect();
    let service_map: Vec<serde_json::Value> = service
        .into_iter()
        .map(|(s, c)| serde_json::json!({"service": s, "count": c}))
        .collect();

    serde_json::json!({
        "severity": {
            "H": severity_map.get("H").unwrap_or(&0),
            "M": severity_map.get("M").unwrap_or(&0),
            "L": severity_map.get("L").unwrap_or(&0),
            "I": severity_map.get("I").unwrap_or(&0),
        },
        "category": {
            "S": category_map.get("S").unwrap_or(&0),
            "C": category_map.get("C").unwrap_or(&0),
            "R": category_map.get("R").unwrap_or(&0),
            "P": category_map.get("P").unwrap_or(&0),
            "O": category_map.get("O").unwrap_or(&0),
        },
        "services": service_map,
    })
}

// ─── GET /api/resources/scans ──────────────────────────────────────────────

/// List scan history
pub async fn list_scans(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
) -> AppResult<Json<Vec<Scan>>> {
    let rows = if auth_user.is_super_admin() {
        sqlx::query_as::<_, Scan>("SELECT * FROM scans ORDER BY created_at DESC LIMIT 50")
            .fetch_all(&state.pool)
            .await?
    } else {
        sqlx::query_as::<_, Scan>(
            "SELECT * FROM scans WHERE tenant_id IS NOT DISTINCT FROM $1 ORDER BY created_at DESC LIMIT 50",
        )
        .bind(auth_user.tenant_id)
        .fetch_all(&state.pool)
        .await?
    };
    Ok(Json(rows))
}

// ─── GET /api/resources/scans/:id ──────────────────────────────────────────

/// Get a specific scan with its summary
pub async fn get_scan(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Scan>> {
    let row = sqlx::query_as::<_, Scan>("SELECT * FROM scans WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Scan not found".to_string()))?;

    if !auth_user.is_super_admin() && row.tenant_id != auth_user.tenant_id {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }

    Ok(Json(row))
}

// ─── GET /api/resources/findings ───────────────────────────────────────────

/// List findings with filtering
pub async fn list_findings(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Query(query): Query<FindingListQuery>,
) -> AppResult<Json<Vec<Finding>>> {
    let limit = query.limit.unwrap_or(200).min(1000);
    let offset = query.offset.unwrap_or(0);

    let rows = if auth_user.is_super_admin() {
        sqlx::query_as::<_, Finding>(
            r#"SELECT * FROM findings
               WHERE ($1::UUID IS NULL OR scan_id = $1)
                 AND ($2::TEXT IS NULL OR severity = $2)
                 AND ($3::TEXT IS NULL OR category = $3)
                 AND ($4::TEXT IS NULL OR service = $4)
                 AND ($5::TEXT IS NULL OR region = $5)
                 AND ($6::TEXT IS NULL OR LOWER(short_desc) LIKE '%' || LOWER($6) || '%'
                      OR LOWER(resource_name) LIKE '%' || LOWER($6) || '%')
               ORDER BY
                 CASE severity WHEN 'H' THEN 1 WHEN 'M' THEN 2 WHEN 'L' THEN 3 ELSE 4 END,
                 created_at DESC
               LIMIT $7 OFFSET $8"#,
        )
        .bind(query.scan_id)
        .bind(&query.severity)
        .bind(&query.category)
        .bind(&query.service)
        .bind(&query.region)
        .bind(&query.q)
        .bind(limit)
        .bind(offset)
        .fetch_all(&state.pool)
        .await?
    } else {
        sqlx::query_as::<_, Finding>(
            r#"SELECT * FROM findings
               WHERE tenant_id IS NOT DISTINCT FROM $1
                 AND ($2::UUID IS NULL OR scan_id = $2)
                 AND ($3::TEXT IS NULL OR severity = $3)
                 AND ($4::TEXT IS NULL OR category = $4)
                 AND ($5::TEXT IS NULL OR service = $5)
                 AND ($6::TEXT IS NULL OR region = $6)
                 AND ($7::TEXT IS NULL OR LOWER(short_desc) LIKE '%' || LOWER($7) || '%'
                      OR LOWER(resource_name) LIKE '%' || LOWER($7) || '%')
               ORDER BY
                 CASE severity WHEN 'H' THEN 1 WHEN 'M' THEN 2 WHEN 'L' THEN 3 ELSE 4 END,
                 created_at DESC
               LIMIT $8 OFFSET $9"#,
        )
        .bind(auth_user.tenant_id)
        .bind(query.scan_id)
        .bind(&query.severity)
        .bind(&query.category)
        .bind(&query.service)
        .bind(&query.region)
        .bind(&query.q)
        .bind(limit)
        .bind(offset)
        .fetch_all(&state.pool)
        .await?
    };

    Ok(Json(rows))
}

// ─── GET /api/resources/dashboard ──────────────────────────────────────────

/// Aggregated dashboard stats
pub async fn dashboard(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
) -> AppResult<Json<DashboardStats>> {
    let tenant_filter = if auth_user.is_super_admin() {
        "TRUE"
    } else {
        "tenant_id IS NOT DISTINCT FROM $1"
    };

    // Get the latest scan's findings for dashboard
    let latest_scan = if auth_user.is_super_admin() {
        sqlx::query_as::<_, Scan>("SELECT * FROM scans WHERE status = 'completed' ORDER BY completed_at DESC LIMIT 1")
            .fetch_optional(&state.pool)
            .await?
    } else {
        sqlx::query_as::<_, Scan>(
            "SELECT * FROM scans WHERE status = 'completed' AND tenant_id IS NOT DISTINCT FROM $1 ORDER BY completed_at DESC LIMIT 1",
        )
        .bind(auth_user.tenant_id)
        .fetch_optional(&state.pool)
        .await?
    };

    // Non-compliant findings only
    let base_where = if auth_user.is_super_admin() {
        "compliant = false".to_string()
    } else {
        format!("compliant = false AND {}", tenant_filter)
    };

    // Total
    let total: (i64,) = if auth_user.is_super_admin() {
        sqlx::query_as(&format!("SELECT COUNT(*) FROM findings WHERE {}", base_where))
            .fetch_one(&state.pool)
            .await
            .unwrap_or((0,))
    } else {
        sqlx::query_as(&format!("SELECT COUNT(*) FROM findings WHERE {}", base_where))
            .bind(auth_user.tenant_id)
            .fetch_one(&state.pool)
            .await
            .unwrap_or((0,))
    };

    // By severity
    let severity_rows: Vec<(String, i64)> = if auth_user.is_super_admin() {
        sqlx::query_as(&format!(
            "SELECT severity, COUNT(*) FROM findings WHERE {} GROUP BY severity",
            base_where
        ))
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default()
    } else {
        sqlx::query_as(&format!(
            "SELECT severity, COUNT(*) FROM findings WHERE {} GROUP BY severity",
            base_where
        ))
        .bind(auth_user.tenant_id)
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default()
    };
    let sev_map: std::collections::HashMap<String, i64> = severity_rows.into_iter().collect();

    // By category
    let cat_rows: Vec<(String, i64)> = if auth_user.is_super_admin() {
        sqlx::query_as(&format!(
            "SELECT category, COUNT(*) FROM findings WHERE {} GROUP BY category",
            base_where
        ))
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default()
    } else {
        sqlx::query_as(&format!(
            "SELECT category, COUNT(*) FROM findings WHERE {} GROUP BY category",
            base_where
        ))
        .bind(auth_user.tenant_id)
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default()
    };
    let cat_map: std::collections::HashMap<String, i64> = cat_rows.into_iter().collect();

    // By service (top 10)
    let by_service: Vec<ServiceCount> = if auth_user.is_super_admin() {
        sqlx::query_as(&format!(
            "SELECT service, COUNT(*) as count FROM findings WHERE {} GROUP BY service ORDER BY count DESC LIMIT 10",
            base_where
        ))
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default()
    } else {
        sqlx::query_as(&format!(
            "SELECT service, COUNT(*) as count FROM findings WHERE {} GROUP BY service ORDER BY count DESC LIMIT 10",
            base_where
        ))
        .bind(auth_user.tenant_id)
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default()
    };

    Ok(Json(DashboardStats {
        total_findings: total.0,
        by_severity: SeverityCounts {
            high: *sev_map.get("H").unwrap_or(&0),
            medium: *sev_map.get("M").unwrap_or(&0),
            low: *sev_map.get("L").unwrap_or(&0),
            info: *sev_map.get("I").unwrap_or(&0),
        },
        by_category: CategoryCounts {
            security: *cat_map.get("S").unwrap_or(&0),
            cost: *cat_map.get("C").unwrap_or(&0),
            reliability: *cat_map.get("R").unwrap_or(&0),
            performance: *cat_map.get("P").unwrap_or(&0),
            operations: *cat_map.get("O").unwrap_or(&0),
        },
        by_service,
        last_scan: latest_scan,
    }))
}

// ─── Keep old list endpoint for backward compatibility ─────────────────────

/// GET /api/resources (backward compat — returns empty)
pub async fn list(_auth_user: axum::Extension<AuthUser>) -> AppResult<Json<Vec<serde_json::Value>>> {
    Ok(Json(vec![]))
}
