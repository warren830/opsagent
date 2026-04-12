use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::middleware::auth::AuthUser;
use crate::models::account_access::{
    AccessibleAccount, GrantAccessRequest, UserAccessView, UserAccountAccess,
};
use crate::models::cloud_account::CloudAccount;

/// Get the list of account IDs the current user can access.
/// Reusable by glossary, knowledge, chat handlers.
pub async fn get_accessible_account_ids(pool: &PgPool, auth_user: &AuthUser) -> Vec<Uuid> {
    if auth_user.is_super_admin() {
        // All accounts
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM cloud_accounts")
            .fetch_all(pool)
            .await
            .unwrap_or_default()
    } else if auth_user.is_tenant_admin() {
        // Accounts in tenant + explicitly granted accounts
        sqlx::query_scalar::<_, Uuid>(
            r#"SELECT DISTINCT id FROM (
                SELECT id FROM cloud_accounts WHERE tenant_id = $1
                UNION
                SELECT account_id FROM user_account_access WHERE user_id = $2
            ) sub"#,
        )
        .bind(auth_user.tenant_id)
        .bind(auth_user.user_id)
        .fetch_all(pool)
        .await
        .unwrap_or_default()
    } else {
        // Only explicitly granted accounts
        sqlx::query_scalar::<_, Uuid>(
            "SELECT account_id FROM user_account_access WHERE user_id = $1",
        )
        .bind(auth_user.user_id)
        .fetch_all(pool)
        .await
        .unwrap_or_default()
    }
}

/// Check if the user has write access to a specific account.
/// Priority: explicit grant role > implicit tenant role > deny
///   - super_admin → always write
///   - Has explicit grant → use grant role (admin=write, readonly=deny)
///   - No explicit grant + tenant_admin (own tenant) → write
///   - Otherwise → deny
pub async fn can_write_account(pool: &PgPool, auth_user: &AuthUser, account_id: Uuid) -> bool {
    if auth_user.is_super_admin() {
        return true;
    }

    // Check explicit grant first — it takes priority over implicit tenant access
    let grant_role = sqlx::query_scalar::<_, String>(
        "SELECT role FROM user_account_access WHERE user_id = $1 AND account_id = $2",
    )
    .bind(auth_user.user_id)
    .bind(account_id)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);

    if let Some(role) = grant_role {
        return role == "admin";
    }

    // No explicit grant — tenant_admin can write to accounts in their tenant
    if auth_user.is_tenant_admin() {
        return sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM cloud_accounts WHERE id = $1 AND tenant_id = $2)",
        )
        .bind(account_id)
        .bind(auth_user.tenant_id)
        .fetch_one(pool)
        .await
        .unwrap_or(false);
    }

    false
}

/// Returns accounts the current user can access, with writable flag.
pub async fn my_accessible_accounts(
    pool: &PgPool,
    auth_user: &AuthUser,
) -> AppResult<Vec<AccessibleAccount>> {
    let mut accounts = if auth_user.is_super_admin() {
        sqlx::query_as::<_, AccessibleAccount>(
            "SELECT id, provider, name, account_id FROM cloud_accounts ORDER BY provider, name",
        )
        .fetch_all(pool)
        .await?
    } else if auth_user.is_tenant_admin() {
        sqlx::query_as::<_, AccessibleAccount>(
            r#"SELECT DISTINCT id, provider, name, account_id FROM (
                SELECT id, provider, name, account_id FROM cloud_accounts WHERE tenant_id = $1
                UNION
                SELECT ca.id, ca.provider, ca.name, ca.account_id
                FROM cloud_accounts ca
                JOIN user_account_access uaa ON ca.id = uaa.account_id
                WHERE uaa.user_id = $2
            ) sub ORDER BY provider, name"#,
        )
        .bind(auth_user.tenant_id)
        .bind(auth_user.user_id)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as::<_, AccessibleAccount>(
            r#"SELECT ca.id, ca.provider, ca.name, ca.account_id
               FROM cloud_accounts ca
               JOIN user_account_access uaa ON ca.id = uaa.account_id
               WHERE uaa.user_id = $1
               ORDER BY ca.provider, ca.name"#,
        )
        .bind(auth_user.user_id)
        .fetch_all(pool)
        .await?
    };

    // Set writable flag per account
    if auth_user.is_super_admin() {
        for a in &mut accounts {
            a.writable = true;
        }
    } else {
        for a in &mut accounts {
            a.writable = can_write_account(pool, auth_user, a.id).await;
        }
    }

    Ok(accounts)
}

/// List users who have access to a specific account.
pub async fn list_account_users(
    pool: &PgPool,
    auth_user: &AuthUser,
    account_id: Uuid,
) -> AppResult<Vec<UserAccessView>> {
    // Verify the account exists and caller has access
    let account = sqlx::query_as::<_, CloudAccount>("SELECT * FROM cloud_accounts WHERE id = $1")
        .bind(account_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Cloud account not found".to_string()))?;

    if !auth_user.is_super_admin()
        && (!auth_user.is_tenant_admin() || account.tenant_id != auth_user.tenant_id)
    {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }

    let users = sqlx::query_as::<_, UserAccessView>(
        r#"SELECT uaa.user_id, u.username, u.email, uaa.role, uaa.created_at
           FROM user_account_access uaa
           JOIN users u ON u.id = uaa.user_id
           WHERE uaa.account_id = $1
           ORDER BY u.username"#,
    )
    .bind(account_id)
    .fetch_all(pool)
    .await?;

    Ok(users)
}

/// Grant a user access to an account (admin only).
pub async fn grant(
    pool: &PgPool,
    auth_user: &AuthUser,
    req: GrantAccessRequest,
) -> AppResult<UserAccountAccess> {
    if !auth_user.is_admin() {
        return Err(AppError::Forbidden(
            "Only admins can grant account access".to_string(),
        ));
    }

    // Validate role
    let role = match req.role.as_str() {
        "admin" | "readonly" => req.role.clone(),
        _ => "readonly".to_string(),
    };

    // Verify the account exists
    let account = sqlx::query_as::<_, CloudAccount>("SELECT * FROM cloud_accounts WHERE id = $1")
        .bind(req.account_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Cloud account not found".to_string()))?;

    // tenant_admin can only grant access to accounts in their tenant
    if auth_user.is_tenant_admin()
        && !auth_user.is_super_admin()
        && account.tenant_id != auth_user.tenant_id
    {
        return Err(AppError::Forbidden(
            "Cannot grant access to accounts outside your tenant".to_string(),
        ));
    }

    // Upsert: insert or update role on conflict
    let access = sqlx::query_as::<_, UserAccountAccess>(
        r#"INSERT INTO user_account_access (user_id, account_id, role)
           VALUES ($1, $2, $3)
           ON CONFLICT (user_id, account_id) DO UPDATE SET role = EXCLUDED.role
           RETURNING *"#,
    )
    .bind(req.user_id)
    .bind(req.account_id)
    .bind(&role)
    .fetch_one(pool)
    .await?;

    Ok(access)
}

/// Revoke a user's access to an account (admin only).
pub async fn revoke(
    pool: &PgPool,
    auth_user: &AuthUser,
    user_id: Uuid,
    account_id: Uuid,
) -> AppResult<()> {
    if !auth_user.is_admin() {
        return Err(AppError::Forbidden(
            "Only admins can revoke account access".to_string(),
        ));
    }

    // tenant_admin: verify account is in their tenant
    if auth_user.is_tenant_admin() && !auth_user.is_super_admin() {
        let account =
            sqlx::query_as::<_, CloudAccount>("SELECT * FROM cloud_accounts WHERE id = $1")
                .bind(account_id)
                .fetch_optional(pool)
                .await?
                .ok_or_else(|| AppError::NotFound("Cloud account not found".to_string()))?;

        if account.tenant_id != auth_user.tenant_id {
            return Err(AppError::Forbidden(
                "Cannot revoke access to accounts outside your tenant".to_string(),
            ));
        }
    }

    let result =
        sqlx::query("DELETE FROM user_account_access WHERE user_id = $1 AND account_id = $2")
            .bind(user_id)
            .bind(account_id)
            .execute(pool)
            .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Access record not found".to_string()));
    }

    Ok(())
}
