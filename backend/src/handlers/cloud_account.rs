use axum::{
    Json,
    extract::{Path, State},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::AppState;
use crate::error::{AppError, AppResult};
use crate::middleware::auth::AuthUser;
use crate::models::cloud_account::{CloudAccount, CreateCloudAccountRequest, UpdateCloudAccountRequest};

/// GET /api/accounts
/// Super admin: all. Tenant admin: tenant's accounts. Normal user: only granted accounts.
pub async fn list(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
) -> AppResult<Json<Vec<CloudAccount>>> {
    let accounts = if auth_user.is_super_admin() {
        sqlx::query_as::<_, CloudAccount>("SELECT * FROM cloud_accounts ORDER BY provider, name")
            .fetch_all(&state.pool)
            .await?
    } else if auth_user.is_tenant_admin() {
        // Accounts in tenant + explicitly granted accounts
        sqlx::query_as::<_, CloudAccount>(
            r#"SELECT DISTINCT ON (id) * FROM (
                SELECT * FROM cloud_accounts WHERE tenant_id = $1
                UNION ALL
                SELECT ca.* FROM cloud_accounts ca
                JOIN user_account_access uaa ON ca.id = uaa.account_id
                WHERE uaa.user_id = $2
            ) sub ORDER BY id, provider, name"#,
        )
        .bind(auth_user.tenant_id)
        .bind(auth_user.user_id)
        .fetch_all(&state.pool)
        .await?
    } else {
        // Normal user: only accounts they have been granted access to
        sqlx::query_as::<_, CloudAccount>(
            r#"SELECT ca.* FROM cloud_accounts ca
               JOIN user_account_access uaa ON ca.id = uaa.account_id
               WHERE uaa.user_id = $1
               ORDER BY ca.provider, ca.name"#,
        )
        .bind(auth_user.user_id)
        .fetch_all(&state.pool)
        .await?
    };

    Ok(Json(accounts))
}

/// POST /api/accounts
pub async fn create(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Json(req): Json<CreateCloudAccountRequest>,
) -> AppResult<Json<CloudAccount>> {
    if req.name.trim().is_empty() {
        return Err(AppError::BadRequest("Name is required".to_string()));
    }

    if req.provider.trim().is_empty() {
        return Err(AppError::BadRequest("Provider is required".to_string()));
    }

    // Super admin can specify tenant_id; normal users use their own
    let tenant_id = if auth_user.is_super_admin() {
        req.tenant_id.or(auth_user.tenant_id)
    } else {
        auth_user.tenant_id
    };
    let regions = req.regions.unwrap_or_else(|| vec!["us-east-1".to_string()]);
    let source = req.source.unwrap_or_else(|| "manual".to_string());

    let account = sqlx::query_as::<_, CloudAccount>(
        r#"INSERT INTO cloud_accounts (provider, name, account_id, config, secret_arn, tenant_id, is_mock, role_arn, profile, regions, source)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
           RETURNING *"#,
    )
    .bind(&req.provider)
    .bind(&req.name)
    .bind(&req.account_id)
    .bind(&req.config)
    .bind(&req.secret_arn)
    .bind(tenant_id)
    .bind(req.is_mock)
    .bind(&req.role_arn)
    .bind(&req.profile)
    .bind(&regions)
    .bind(&source)
    .fetch_one(&state.pool)
    .await?;

    // If requested, discover Organization accounts using this account's credentials
    if req.discover_org && req.provider == "aws" {
        let profile = req.profile.clone();
        let pool = state.pool.clone();
        let tid = tenant_id;
        tokio::spawn(async move {
            let env_vars: Vec<(String, String)> = match &profile {
                Some(p) => vec![("AWS_PROFILE".to_string(), p.clone())],
                None => vec![],
            };
            if let Ok(org_output) = try_list_org_accounts(&env_vars).await {
                for org_account in &org_output.accounts {
                    if org_account.status.as_deref().is_some_and(|s| s == "SUSPENDED") {
                        continue;
                    }
                    // Skip if already exists (any source)
                    let exists = sqlx::query_scalar::<_, bool>(
                        "SELECT EXISTS(SELECT 1 FROM cloud_accounts WHERE account_id = $1 AND tenant_id IS NOT DISTINCT FROM $2)",
                    )
                    .bind(&org_account.id)
                    .bind(tid)
                    .fetch_one(&pool)
                    .await
                    .unwrap_or(false);
                    if exists {
                        continue;
                    }
                    let role_arn = format!("arn:aws:iam::{}:role/OrganizationAccountAccessRole", org_account.id);
                    let _ = sqlx::query(
                        r#"INSERT INTO cloud_accounts (provider, name, account_id, role_arn, regions, tenant_id, source, config)
                           VALUES ('aws', $1, $2, $3, '{}'::text[], $4, 'organization', '{}')
                           ON CONFLICT (tenant_id, account_id) WHERE source = 'organization'
                           DO UPDATE SET name = EXCLUDED.name, role_arn = EXCLUDED.role_arn, updated_at = NOW()"#,
                    )
                    .bind(&org_account.name)
                    .bind(&org_account.id)
                    .bind(&role_arn)
                    .bind(tid)
                    .execute(&pool)
                    .await;
                }
            }
        });
    }

    Ok(Json(account))
}

/// PUT /api/accounts/:id
pub async fn update(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateCloudAccountRequest>,
) -> AppResult<Json<CloudAccount>> {
    if !auth_user.is_super_admin() {
        let accessible = crate::handlers::account_access::get_accessible_account_ids(&state.pool, &auth_user).await;
        if !accessible.contains(&id) {
            return Err(AppError::Forbidden("Access denied".to_string()));
        }
    }

    let account = sqlx::query_as::<_, CloudAccount>(
        r#"UPDATE cloud_accounts SET
           provider = COALESCE($2, provider),
           name = COALESCE($3, name),
           account_id = COALESCE($4, account_id),
           config = COALESCE($5, config),
           secret_arn = COALESCE($6, secret_arn),
           is_mock = COALESCE($7, is_mock),
           role_arn = COALESCE($8, role_arn),
           profile = COALESCE($9, profile),
           regions = COALESCE($10, regions),
           tenant_id = $11,
           updated_at = NOW()
           WHERE id = $1
           RETURNING *"#,
    )
    .bind(id)
    .bind(&req.provider)
    .bind(&req.name)
    .bind(&req.account_id)
    .bind(&req.config)
    .bind(&req.secret_arn)
    .bind(req.is_mock)
    .bind(&req.role_arn)
    .bind(&req.profile)
    .bind(&req.regions)
    .bind(req.tenant_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Cloud account not found".to_string()))?;

    Ok(Json(account))
}

/// DELETE /api/accounts/:id
pub async fn delete(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    if !auth_user.is_super_admin() {
        let accessible = crate::handlers::account_access::get_accessible_account_ids(&state.pool, &auth_user).await;
        if !accessible.contains(&id) {
            return Err(AppError::Forbidden("Access denied".to_string()));
        }
    }

    let result = sqlx::query("DELETE FROM cloud_accounts WHERE id = $1")
        .bind(id)
        .execute(&state.pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Cloud account not found".to_string()));
    }

    Ok(Json(serde_json::json!({"message": "Cloud account deleted"})))
}

// ─── Organization Discovery ─────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct OrgAccount {
    #[serde(rename = "Id")]
    id: String,
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Status")]
    status: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OrgListOutput {
    #[serde(rename = "Accounts")]
    accounts: Vec<OrgAccount>,
}

/// POST /api/accounts/discover
/// Discover AWS accounts from Organizations using `aws organizations list-accounts`.
/// Tries each existing AWS account's profile, then falls back to default credentials.
pub async fn discover(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
) -> AppResult<Json<Vec<CloudAccount>>> {
    let tenant_id = auth_user.tenant_id;

    // Get existing AWS accounts to borrow their profiles
    let existing_accounts = sqlx::query_as::<_, CloudAccount>(
        "SELECT * FROM cloud_accounts WHERE provider = 'aws' AND is_mock = false AND tenant_id = $1 ORDER BY created_at",
    )
    .bind(tenant_id)
    .fetch_all(&state.pool)
    .await?;

    let mut org_output: Option<OrgListOutput> = None;

    // Try each account's profile
    for account in &existing_accounts {
        if let Some(ref profile) = account.profile
            && let Ok(output) = try_list_org_accounts(&[("AWS_PROFILE".to_string(), profile.clone())]).await
        {
            org_output = Some(output);
            break;
        }
    }
    // Fallback to default credentials
    if org_output.is_none()
        && let Ok(output) = try_list_org_accounts(&[]).await
    {
        org_output = Some(output);
    }

    let org_output = org_output.ok_or_else(|| {
        AppError::Internal("Could not list Organization accounts with any available credentials".to_string())
    })?;

    let mut results = Vec::new();

    for org_account in &org_output.accounts {
        if org_account.status.as_deref().is_some_and(|s| s == "SUSPENDED") {
            continue;
        }

        // Skip if already exists (any source)
        let existing = sqlx::query_as::<_, CloudAccount>(
            "SELECT * FROM cloud_accounts WHERE account_id = $1 AND tenant_id IS NOT DISTINCT FROM $2",
        )
        .bind(&org_account.id)
        .bind(tenant_id)
        .fetch_optional(&state.pool)
        .await;

        if let Ok(Some(a)) = existing {
            results.push(a);
            continue;
        }

        let role_arn = format!("arn:aws:iam::{}:role/OrganizationAccountAccessRole", org_account.id);

        let account = sqlx::query_as::<_, CloudAccount>(
            r#"INSERT INTO cloud_accounts (provider, name, account_id, role_arn, regions, tenant_id, source, config)
               VALUES ('aws', $1, $2, $3, '{}'::text[], $4, 'organization', '{}')
               ON CONFLICT (tenant_id, account_id) WHERE source = 'organization'
               DO UPDATE SET name = EXCLUDED.name, role_arn = EXCLUDED.role_arn, updated_at = NOW()
               RETURNING *"#,
        )
        .bind(&org_account.name)
        .bind(&org_account.id)
        .bind(&role_arn)
        .bind(tenant_id)
        .fetch_optional(&state.pool)
        .await;

        match account {
            Ok(Some(a)) => results.push(a),
            Err(e) => {
                tracing::warn!("Failed to upsert org account {}: {}", org_account.id, e);
            }
            _ => {}
        }
    }

    Ok(Json(results))
}

/// Try calling `aws organizations list-accounts` with given env vars
async fn try_list_org_accounts(env_vars: &[(String, String)]) -> Result<OrgListOutput, String> {
    let mut cmd = tokio::process::Command::new("aws");
    cmd.args(["organizations", "list-accounts", "--output", "json"]);
    for (k, v) in env_vars {
        cmd.env(k, v);
    }
    let output = cmd.output().await.map_err(|e| format!("Failed to run aws CLI: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(stderr.trim().to_string());
    }
    serde_json::from_slice(&output.stdout).map_err(|e| format!("Failed to parse response: {e}"))
}

/// POST /api/accounts/:id/discover-org
/// Discover Organization accounts using a specific account's profile.
pub async fn discover_org(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Vec<CloudAccount>>> {
    let account = sqlx::query_as::<_, CloudAccount>("SELECT * FROM cloud_accounts WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Cloud account not found".to_string()))?;

    if !auth_user.is_super_admin() && account.tenant_id != auth_user.tenant_id {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }
    if account.provider != "aws" {
        return Err(AppError::BadRequest(
            "Organization discovery only supported for AWS accounts".to_string(),
        ));
    }

    // Use the source account's tenant_id so discovered accounts land in the same tenant
    let tenant_id = account.tenant_id;

    let env_vars: Vec<(String, String)> = match &account.profile {
        Some(p) => vec![("AWS_PROFILE".to_string(), p.clone())],
        None => vec![],
    };

    let org_output = try_list_org_accounts(&env_vars)
        .await
        .map_err(|e| AppError::Internal(format!("AWS Organizations error: {}", e)))?;

    let mut results = Vec::new();

    for org_account in &org_output.accounts {
        if org_account.status.as_deref().is_some_and(|s| s == "SUSPENDED") {
            continue;
        }

        // Skip if this account_id already exists (any source, any tenant — use IS NOT DISTINCT FROM for NULL-safe compare)
        let existing = sqlx::query_as::<_, CloudAccount>(
            "SELECT * FROM cloud_accounts WHERE account_id = $1 AND tenant_id IS NOT DISTINCT FROM $2",
        )
        .bind(&org_account.id)
        .bind(tenant_id)
        .fetch_optional(&state.pool)
        .await;

        if let Ok(Some(a)) = existing {
            results.push(a);
            continue;
        }

        // Auto-fill role_arn so the root account can assume into child accounts
        let role_arn = format!("arn:aws:iam::{}:role/OrganizationAccountAccessRole", org_account.id);

        let row = sqlx::query_as::<_, CloudAccount>(
            r#"INSERT INTO cloud_accounts (provider, name, account_id, role_arn, regions, tenant_id, source, config)
               VALUES ('aws', $1, $2, $3, '{}'::text[], $4, 'organization', '{}')
               ON CONFLICT (tenant_id, account_id) WHERE source = 'organization'
               DO UPDATE SET name = EXCLUDED.name, role_arn = EXCLUDED.role_arn, updated_at = NOW()
               RETURNING *"#,
        )
        .bind(&org_account.name)
        .bind(&org_account.id)
        .bind(&role_arn)
        .bind(tenant_id)
        .fetch_optional(&state.pool)
        .await;

        match row {
            Ok(Some(a)) => results.push(a),
            Err(e) => {
                tracing::warn!("Failed to upsert org account {}: {}", org_account.id, e);
            }
            _ => {}
        }
    }

    Ok(Json(results))
}

// ─── Test Connection ────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct TestConnectionResult {
    pub success: bool,
    pub identity: Option<String>,
    pub error: Option<String>,
}

/// POST /api/accounts/:id/test
/// Test connection to a cloud account using `aws sts get-caller-identity`
pub async fn test_connection(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<TestConnectionResult>> {
    let account = sqlx::query_as::<_, CloudAccount>("SELECT * FROM cloud_accounts WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Cloud account not found".to_string()))?;

    // Check tenant access
    if !auth_user.is_super_admin() && account.tenant_id != auth_user.tenant_id {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }

    if account.provider != "aws" {
        return Ok(Json(TestConnectionResult {
            success: false,
            identity: None,
            error: Some(format!("Test not supported for provider: {}", account.provider)),
        }));
    }

    // Build aws CLI command with appropriate credentials
    let mut cmd = tokio::process::Command::new("aws");
    cmd.args(["sts", "get-caller-identity", "--output", "json"]);

    if let Some(ref profile) = account.profile {
        cmd.args(["--profile", profile]);
    }

    if let Some(ref role_arn) = account.role_arn {
        // For org-discovered accounts, assume-role needs the root account's profile.
        // Find a profile from the same tenant to use as the caller identity.
        let caller_profile: Option<String> = if account.profile.is_none() {
            sqlx::query_scalar::<_, String>(
                "SELECT profile FROM cloud_accounts WHERE provider = 'aws' AND profile IS NOT NULL AND tenant_id IS NOT DISTINCT FROM $1 LIMIT 1",
            )
            .bind(account.tenant_id)
            .fetch_optional(&state.pool)
            .await
            .ok()
            .flatten()
        } else {
            account.profile.clone()
        };

        let mut assume_cmd = tokio::process::Command::new("aws");
        assume_cmd.args([
            "sts",
            "assume-role",
            "--role-arn",
            role_arn,
            "--role-session-name",
            "openops-test",
            "--duration-seconds",
            "900",
            "--output",
            "json",
        ]);
        if let Some(ref profile) = caller_profile {
            assume_cmd.args(["--profile", profile]);
        }

        let assume_output = assume_cmd.output().await;

        match assume_output {
            Ok(out) if out.status.success() => {
                let body: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_default();
                let arn = body
                    .pointer("/AssumedRoleUser/Arn")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                return Ok(Json(TestConnectionResult {
                    success: true,
                    identity: Some(format!("AssumedRole: {}", arn)),
                    error: None,
                }));
            }
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                return Ok(Json(TestConnectionResult {
                    success: false,
                    identity: None,
                    error: Some(stderr.trim().to_string()),
                }));
            }
            Err(e) => {
                return Ok(Json(TestConnectionResult {
                    success: false,
                    identity: None,
                    error: Some(format!("Failed to run aws CLI: {}", e)),
                }));
            }
        }
    }

    // Default: just test with current credentials / profile
    let output = cmd.output().await;

    match output {
        Ok(out) if out.status.success() => {
            let body: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_default();
            let arn = body.get("Arn").and_then(|v| v.as_str()).unwrap_or("unknown");
            Ok(Json(TestConnectionResult {
                success: true,
                identity: Some(arn.to_string()),
                error: None,
            }))
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            Ok(Json(TestConnectionResult {
                success: false,
                identity: None,
                error: Some(stderr.trim().to_string()),
            }))
        }
        Err(e) => Ok(Json(TestConnectionResult {
            success: false,
            identity: None,
            error: Some(format!("Failed to run aws CLI: {}", e)),
        })),
    }
}

/// POST /api/accounts/seed-mock
/// Creates mock Alicloud and Azure accounts for the current tenant
pub async fn seed_mock(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
) -> AppResult<Json<Vec<CloudAccount>>> {
    let tenant_id = auth_user.tenant_id;

    let alicloud = sqlx::query_as::<_, CloudAccount>(
        r#"INSERT INTO cloud_accounts (provider, name, account_id, config, tenant_id, is_mock)
           VALUES ('alicloud', 'Alicloud China (Mock)', '1234567890', '{"region": "cn-hangzhou", "mode": "mock"}', $1, true)
           ON CONFLICT DO NOTHING
           RETURNING *"#,
    )
    .bind(tenant_id)
    .fetch_optional(&state.pool)
    .await?;

    let azure = sqlx::query_as::<_, CloudAccount>(
        r#"INSERT INTO cloud_accounts (provider, name, account_id, config, tenant_id, is_mock)
           VALUES ('azure', 'Azure Global (Mock)', 'sub-mock-001', '{"subscription_id": "sub-mock-001", "mode": "mock"}', $1, true)
           ON CONFLICT DO NOTHING
           RETURNING *"#,
    )
    .bind(tenant_id)
    .fetch_optional(&state.pool)
    .await?;

    let mut results = Vec::new();
    if let Some(a) = alicloud {
        results.push(a);
    }
    if let Some(a) = azure {
        results.push(a);
    }

    Ok(Json(results))
}
