use sqlx::PgPool;

use crate::error::AppResult;
use crate::middleware::auth::AuthUser;
use crate::models::dashboard::DashboardStats;

/// Return aggregated dashboard statistics.
/// Super admins see global counts; other users see tenant/user-scoped counts.
pub async fn stats(pool: &PgPool, auth_user: &AuthUser) -> AppResult<DashboardStats> {
    let stats = if auth_user.is_super_admin() {
        // Super admin sees everything
        let tenants: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM tenants")
            .fetch_one(pool)
            .await?;
        let users: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
            .fetch_one(pool)
            .await?;
        let skills: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM skills")
            .fetch_one(pool)
            .await?;
        let clusters: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM clusters")
            .fetch_one(pool)
            .await?;
        let issues_open: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM issues WHERE status IN ('open', 'investigating')",
        )
        .fetch_one(pool)
        .await?;
        let active_sessions: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM claude_sessions ")
                .fetch_one(pool)
                .await?;

        DashboardStats {
            tenants: tenants.0,
            users: users.0,
            skills: skills.0,
            clusters: clusters.0,
            issues_open: issues_open.0,
            active_sessions: active_sessions.0,
        }
    } else {
        // Tenant-scoped stats — user sees own private + tenant public resources
        let tid = auth_user.tenant_id;
        let uid = auth_user.user_id;
        let tenants: (i64,) = if tid.is_some() { (1,) } else { (0,) };
        let users: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM users WHERE tenant_id = $1")
                .bind(tid)
                .fetch_one(pool)
                .await?;
        let skills: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM skills WHERE (user_id = $1) OR (user_id IS NULL AND tenant_id IS NOT DISTINCT FROM $2)",
        )
        .bind(uid)
        .bind(tid)
        .fetch_one(pool)
        .await?;
        let clusters: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM clusters WHERE tenant_id = $1")
                .bind(tid)
                .fetch_one(pool)
                .await?;
        let issues_open: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM issues WHERE tenant_id = $1 AND status IN ('open', 'investigating')",
        )
        .bind(tid)
        .fetch_one(pool)
        .await?;
        let active_sessions: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM claude_sessions WHERE user_id = $1")
                .bind(uid)
                .fetch_one(pool)
                .await?;

        DashboardStats {
            tenants: tenants.0,
            users: users.0,
            skills: skills.0,
            clusters: clusters.0,
            issues_open: issues_open.0,
            active_sessions: active_sessions.0,
        }
    };

    Ok(stats)
}
