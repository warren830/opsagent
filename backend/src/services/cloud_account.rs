use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::middleware::auth::AuthUser;
use crate::models::cloud_account::{CloudAccount, CreateCloudAccountRequest, UpdateCloudAccountRequest};

// ─── Internal types ────────────────────────────────────────────────────────

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

// ─── Public types ──────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct OrgSyncResult {
    pub added: usize,
    pub removed: usize,
    pub updated: usize,
}

#[derive(Debug, Serialize)]
pub struct TestConnectionResult {
    pub success: bool,
    pub identity: Option<String>,
    pub error: Option<String>,
}

// ─── Service functions ─────────────────────────────────────────────────────

/// List cloud accounts visible to the authenticated user.
/// Super admin: all. Tenant admin: tenant + granted (DISTINCT ON). Member: only granted.
pub async fn list(pool: &PgPool, auth_user: &AuthUser) -> AppResult<Vec<CloudAccount>> {
    let accounts = if auth_user.is_super_admin() {
        sqlx::query_as::<_, CloudAccount>("SELECT * FROM cloud_accounts ORDER BY provider, name")
            .fetch_all(pool)
            .await?
    } else if auth_user.is_tenant_admin() {
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
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as::<_, CloudAccount>(
            r#"SELECT ca.* FROM cloud_accounts ca
               JOIN user_account_access uaa ON ca.id = uaa.account_id
               WHERE uaa.user_id = $1
               ORDER BY ca.provider, ca.name"#,
        )
        .bind(auth_user.user_id)
        .fetch_all(pool)
        .await?
    };

    Ok(accounts)
}

/// Create a new cloud account. Returns the account only (does NOT spawn org discovery).
pub async fn create(
    pool: &PgPool,
    auth_user: &AuthUser,
    req: CreateCloudAccountRequest,
) -> AppResult<CloudAccount> {
    // Only admins (super_admin or tenant_admin) can create accounts
    if !auth_user.is_admin() {
        return Err(AppError::Forbidden(
            "Only admins can create cloud accounts".to_string(),
        ));
    }

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
    .fetch_one(pool)
    .await?;

    Ok(account)
}

/// Fire-and-forget org discovery after creating an account.
/// Intended to be called inside `tokio::spawn` from the handler.
pub async fn discover_org_background(pool: &PgPool, profile: Option<&str>, tenant_id: Option<Uuid>) {
    let env_vars: Vec<(String, String)> = match profile {
        Some(p) => vec![("AWS_PROFILE".to_string(), p.to_string())],
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
            .bind(tenant_id)
            .fetch_one(pool)
            .await
            .unwrap_or(false);
            if exists {
                continue;
            }
            let role_arn = format!(
                "arn:aws:iam::{}:role/OrganizationAccountAccessRole",
                org_account.id
            );
            let _ = sqlx::query(
                r#"INSERT INTO cloud_accounts (provider, name, account_id, role_arn, regions, tenant_id, source, config)
                   VALUES ('aws', $1, $2, $3, '{}'::text[], $4, 'organization', '{}')
                   ON CONFLICT (COALESCE(tenant_id, '00000000-0000-0000-0000-000000000000'::uuid), account_id) WHERE source = 'organization'
                   DO UPDATE SET name = EXCLUDED.name, role_arn = EXCLUDED.role_arn, updated_at = NOW()"#,
            )
            .bind(&org_account.name)
            .bind(&org_account.id)
            .bind(&role_arn)
            .bind(tenant_id)
            .execute(pool)
            .await;
        }
    }
}

/// Update a cloud account (with write permission check).
pub async fn update(
    pool: &PgPool,
    auth_user: &AuthUser,
    id: Uuid,
    req: UpdateCloudAccountRequest,
) -> AppResult<CloudAccount> {
    if !crate::services::account_access::can_write_account(pool, auth_user, id).await {
        return Err(AppError::Forbidden("Access denied".to_string()));
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
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Cloud account not found".to_string()))?;

    Ok(account)
}

/// Delete a cloud account (with write permission check).
pub async fn delete(pool: &PgPool, auth_user: &AuthUser, id: Uuid) -> AppResult<()> {
    if !crate::services::account_access::can_write_account(pool, auth_user, id).await {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }

    let result = sqlx::query("DELETE FROM cloud_accounts WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Cloud account not found".to_string()));
    }

    Ok(())
}

/// Discover AWS Organization accounts for the caller's tenant.
/// Tries each existing account's profile, then falls back to default credentials.
pub async fn discover(pool: &PgPool, auth_user: &AuthUser) -> AppResult<Vec<CloudAccount>> {
    let tenant_id = auth_user.tenant_id;

    // Get existing AWS accounts to borrow their profiles
    let existing_accounts = sqlx::query_as::<_, CloudAccount>(
        "SELECT * FROM cloud_accounts WHERE provider = 'aws' AND is_mock = false AND tenant_id = $1 ORDER BY created_at",
    )
    .bind(tenant_id)
    .fetch_all(pool)
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
        AppError::Internal(
            "Could not list Organization accounts with any available credentials".to_string(),
        )
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
        .fetch_optional(pool)
        .await;

        if let Ok(Some(a)) = existing {
            results.push(a);
            continue;
        }

        let role_arn = format!(
            "arn:aws:iam::{}:role/OrganizationAccountAccessRole",
            org_account.id
        );

        let account = sqlx::query_as::<_, CloudAccount>(
            r#"INSERT INTO cloud_accounts (provider, name, account_id, role_arn, regions, tenant_id, source, config)
               VALUES ('aws', $1, $2, $3, '{}'::text[], $4, 'organization', '{}')
               ON CONFLICT (COALESCE(tenant_id, '00000000-0000-0000-0000-000000000000'::uuid), account_id) WHERE source = 'organization'
               DO UPDATE SET name = EXCLUDED.name, role_arn = EXCLUDED.role_arn, updated_at = NOW()
               RETURNING *"#,
        )
        .bind(&org_account.name)
        .bind(&org_account.id)
        .bind(&role_arn)
        .bind(tenant_id)
        .fetch_optional(pool)
        .await;

        match account {
            Ok(Some(a)) => results.push(a),
            Err(e) => {
                tracing::warn!("Failed to upsert org account {}: {}", org_account.id, e);
            }
            _ => {}
        }
    }

    Ok(results)
}

/// Discover Organization accounts using a specific account's profile.
pub async fn discover_org(
    pool: &PgPool,
    auth_user: &AuthUser,
    id: Uuid,
) -> AppResult<Vec<CloudAccount>> {
    let account = sqlx::query_as::<_, CloudAccount>("SELECT * FROM cloud_accounts WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
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

        // Skip if this account_id already exists
        let existing = sqlx::query_as::<_, CloudAccount>(
            "SELECT * FROM cloud_accounts WHERE account_id = $1 AND tenant_id IS NOT DISTINCT FROM $2",
        )
        .bind(&org_account.id)
        .bind(tenant_id)
        .fetch_optional(pool)
        .await;

        if let Ok(Some(a)) = existing {
            results.push(a);
            continue;
        }

        let role_arn = format!(
            "arn:aws:iam::{}:role/OrganizationAccountAccessRole",
            org_account.id
        );

        let row = sqlx::query_as::<_, CloudAccount>(
            r#"INSERT INTO cloud_accounts (provider, name, account_id, role_arn, regions, tenant_id, source, config)
               VALUES ('aws', $1, $2, $3, '{}'::text[], $4, 'organization', '{}')
               ON CONFLICT (COALESCE(tenant_id, '00000000-0000-0000-0000-000000000000'::uuid), account_id) WHERE source = 'organization'
               DO UPDATE SET name = EXCLUDED.name, role_arn = EXCLUDED.role_arn, updated_at = NOW()
               RETURNING *"#,
        )
        .bind(&org_account.name)
        .bind(&org_account.id)
        .bind(&role_arn)
        .bind(tenant_id)
        .fetch_optional(pool)
        .await;

        match row {
            Ok(Some(a)) => results.push(a),
            Err(e) => {
                tracing::warn!("Failed to upsert org account {}: {}", org_account.id, e);
            }
            _ => {}
        }
    }

    Ok(results)
}

/// Core Organization sync logic — callable from both the handler and the background scheduler.
/// For a given tenant (or all tenants when `tenant_id` is None), query AWS Organizations
/// and reconcile the DB: add new accounts, update changed names, remove stale accounts
/// (source = 'organization' only).
pub async fn sync_org_accounts(
    pool: &PgPool,
    tenant_id: Option<Uuid>,
) -> Result<OrgSyncResult, String> {
    // Collect (tenant_id, profile) pairs to try
    let rows: Vec<(Option<Uuid>, Option<String>)> = if let Some(tid) = tenant_id {
        sqlx::query_as(
            "SELECT DISTINCT tenant_id, profile FROM cloud_accounts \
             WHERE provider = 'aws' AND is_mock = false AND profile IS NOT NULL AND tenant_id = $1",
        )
        .bind(tid)
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?
    } else {
        sqlx::query_as(
            "SELECT DISTINCT tenant_id, profile FROM cloud_accounts \
             WHERE provider = 'aws' AND is_mock = false AND profile IS NOT NULL",
        )
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?
    };

    // Deduplicate by tenant_id — one profile per tenant is enough
    let mut seen_tenants = std::collections::HashSet::new();
    let mut tenant_profiles: Vec<(Option<Uuid>, String)> = Vec::new();
    for (tid, profile) in rows {
        if let Some(ref p) = profile
            && seen_tenants.insert(tid)
        {
            tenant_profiles.push((tid, p.clone()));
        }
    }

    // Also try default credentials for the requested tenant (or all)
    if let Some(tid) = tenant_id
        && !seen_tenants.contains(&Some(tid))
    {
        tenant_profiles.push((Some(tid), String::new()));
    }

    if tenant_profiles.is_empty() {
        return Ok(OrgSyncResult {
            added: 0,
            removed: 0,
            updated: 0,
        });
    }

    let mut total_added = 0usize;
    let mut total_removed = 0usize;
    let mut total_updated = 0usize;

    for (tid, profile) in &tenant_profiles {
        let env_vars: Vec<(String, String)> = if profile.is_empty() {
            vec![]
        } else {
            vec![("AWS_PROFILE".to_string(), profile.clone())]
        };

        let org_output = match try_list_org_accounts(&env_vars).await {
            Ok(o) => o,
            Err(e) => {
                tracing::warn!(
                    "Org sync failed for tenant {:?} profile {:?}: {}",
                    tid,
                    profile,
                    e
                );
                continue;
            }
        };

        // Build set of active org account IDs
        let active_ids: std::collections::HashSet<String> = org_output
            .accounts
            .iter()
            .filter(|a| a.status.as_deref() != Some("SUSPENDED"))
            .map(|a| a.id.clone())
            .collect();

        // Determine the management account ID (the one running the org list)
        // so we can skip creating an org-sync entry for it (it already has a manual entry with profile)
        let mgmt_account_id: Option<String> = {
            let mut cmd = tokio::process::Command::new("aws");
            cmd.args(["sts", "get-caller-identity", "--query", "Account", "--output", "text"]);
            for (k, v) in &env_vars {
                cmd.env(k, v);
            }
            cmd.output().await.ok().and_then(|o| {
                if o.status.success() {
                    Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
                } else {
                    None
                }
            })
        };

        // Upsert active accounts (skip the management account — it has a manual entry with profile)
        for org_account in &org_output.accounts {
            if org_account.status.as_deref() == Some("SUSPENDED") {
                continue;
            }

            // Skip management account — it already has a manual entry with SSO profile
            if mgmt_account_id.as_deref() == Some(&org_account.id) {
                continue;
            }

            let role_arn = format!(
                "arn:aws:iam::{}:role/OrganizationAccountAccessRole",
                org_account.id
            );

            let result = sqlx::query(
                r#"INSERT INTO cloud_accounts (provider, name, account_id, role_arn, regions, tenant_id, source, config)
                   VALUES ('aws', $1, $2, $3, '{}'::text[], $4, 'organization', '{}')
                   ON CONFLICT (COALESCE(tenant_id, '00000000-0000-0000-0000-000000000000'::uuid), account_id) WHERE source = 'organization'
                   DO UPDATE SET name = EXCLUDED.name, role_arn = EXCLUDED.role_arn, updated_at = NOW()
                   RETURNING (xmax = 0) AS is_insert"#,
            )
            .bind(&org_account.name)
            .bind(&org_account.id)
            .bind(&role_arn)
            .bind(*tid)
            .fetch_optional(pool)
            .await;

            match result {
                Ok(Some(row)) => {
                    use sqlx::Row;
                    let is_insert: bool = row.try_get("is_insert").unwrap_or(true);
                    if is_insert {
                        total_added += 1;
                    } else {
                        total_updated += 1;
                    }
                }
                Err(e) => {
                    tracing::warn!("Org sync upsert failed for {}: {}", org_account.id, e);
                }
                _ => {}
            }
        }

        // Remove organization-sourced accounts that are no longer in the org
        let stale = sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM cloud_accounts \
             WHERE source = 'organization' AND provider = 'aws' \
             AND tenant_id IS NOT DISTINCT FROM $1 \
             AND account_id != ALL($2)",
        )
        .bind(*tid)
        .bind(active_ids.iter().cloned().collect::<Vec<String>>())
        .fetch_all(pool)
        .await
        .unwrap_or_default();

        if !stale.is_empty() {
            let deleted = sqlx::query("DELETE FROM cloud_accounts WHERE id = ANY($1) AND source = 'organization'")
                .bind(&stale)
                .execute(pool)
                .await
                .map(|r| r.rows_affected())
                .unwrap_or(0);
            total_removed += deleted as usize;
        }
    }

    tracing::info!(
        "Org sync complete: added={}, updated={}, removed={}",
        total_added,
        total_updated,
        total_removed
    );

    Ok(OrgSyncResult {
        added: total_added,
        removed: total_removed,
        updated: total_updated,
    })
}

/// Test connection to a cloud account using `aws sts get-caller-identity`.
pub async fn test_connection(
    pool: &PgPool,
    auth_user: &AuthUser,
    id: Uuid,
) -> AppResult<TestConnectionResult> {
    let account = sqlx::query_as::<_, CloudAccount>("SELECT * FROM cloud_accounts WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Cloud account not found".to_string()))?;

    // Check tenant access
    if !auth_user.is_super_admin() && account.tenant_id != auth_user.tenant_id {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }

    if account.provider != "aws" {
        return Ok(TestConnectionResult {
            success: false,
            identity: None,
            error: Some(format!("Test not supported for provider: {}", account.provider)),
        });
    }

    // Build aws CLI command with appropriate credentials
    let mut cmd = tokio::process::Command::new("aws");
    cmd.args(["sts", "get-caller-identity", "--output", "json"]);

    if let Some(ref profile) = account.profile {
        cmd.args(["--profile", profile]);
    }

    if let Some(ref role_arn) = account.role_arn {
        // For org-discovered accounts, assume-role needs the root account's profile.
        let caller_profile: Option<String> = if account.profile.is_none() {
            sqlx::query_scalar::<_, String>(
                "SELECT profile FROM cloud_accounts WHERE provider = 'aws' AND profile IS NOT NULL AND tenant_id IS NOT DISTINCT FROM $1 LIMIT 1",
            )
            .bind(account.tenant_id)
            .fetch_optional(pool)
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
                let body: serde_json::Value =
                    serde_json::from_slice(&out.stdout).unwrap_or_default();
                let arn = body
                    .pointer("/AssumedRoleUser/Arn")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                return Ok(TestConnectionResult {
                    success: true,
                    identity: Some(format!("AssumedRole: {}", arn)),
                    error: None,
                });
            }
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                return Ok(TestConnectionResult {
                    success: false,
                    identity: None,
                    error: Some(stderr.trim().to_string()),
                });
            }
            Err(e) => {
                return Ok(TestConnectionResult {
                    success: false,
                    identity: None,
                    error: Some(format!("Failed to run aws CLI: {}", e)),
                });
            }
        }
    }

    // Default: just test with current credentials / profile
    let output = cmd.output().await;

    match output {
        Ok(out) if out.status.success() => {
            let body: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_default();
            let arn = body.get("Arn").and_then(|v| v.as_str()).unwrap_or("unknown");
            Ok(TestConnectionResult {
                success: true,
                identity: Some(arn.to_string()),
                error: None,
            })
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            Ok(TestConnectionResult {
                success: false,
                identity: None,
                error: Some(stderr.trim().to_string()),
            })
        }
        Err(e) => Ok(TestConnectionResult {
            success: false,
            identity: None,
            error: Some(format!("Failed to run aws CLI: {}", e)),
        }),
    }
}

/// Insert mock Alicloud and Azure accounts for the current tenant.
pub async fn seed_mock(pool: &PgPool, auth_user: &AuthUser) -> AppResult<Vec<CloudAccount>> {
    let tenant_id = auth_user.tenant_id;

    let alicloud = sqlx::query_as::<_, CloudAccount>(
        r#"INSERT INTO cloud_accounts (provider, name, account_id, config, tenant_id, is_mock)
           VALUES ('alicloud', 'Alicloud China (Mock)', '1234567890', '{"region": "cn-hangzhou", "mode": "mock"}', $1, true)
           ON CONFLICT DO NOTHING
           RETURNING *"#,
    )
    .bind(tenant_id)
    .fetch_optional(pool)
    .await?;

    let azure = sqlx::query_as::<_, CloudAccount>(
        r#"INSERT INTO cloud_accounts (provider, name, account_id, config, tenant_id, is_mock)
           VALUES ('azure', 'Azure Global (Mock)', 'sub-mock-001', '{"subscription_id": "sub-mock-001", "mode": "mock"}', $1, true)
           ON CONFLICT DO NOTHING
           RETURNING *"#,
    )
    .bind(tenant_id)
    .fetch_optional(pool)
    .await?;

    let mut results = Vec::new();
    if let Some(a) = alicloud {
        results.push(a);
    }
    if let Some(a) = azure {
        results.push(a);
    }

    Ok(results)
}

/// Try calling `aws organizations list-accounts` with given env vars.
async fn try_list_org_accounts(env_vars: &[(String, String)]) -> Result<OrgListOutput, String> {
    let mut cmd = tokio::process::Command::new("aws");
    cmd.args(["organizations", "list-accounts", "--output", "json"]);
    for (k, v) in env_vars {
        cmd.env(k, v);
    }
    let output = cmd
        .output()
        .await
        .map_err(|e| format!("Failed to run aws CLI: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(stderr.trim().to_string());
    }
    serde_json::from_slice(&output.stdout).map_err(|e| format!("Failed to parse response: {e}"))
}
