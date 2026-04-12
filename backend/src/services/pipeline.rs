use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::middleware::auth::AuthUser;
use crate::models::pipeline::{CreatePipelineRepoRequest, PipelineRepo, UpdatePipelineRepoRequest};
use crate::services::common::require_non_empty;

#[derive(Debug, Serialize)]
pub struct TestConnectionResult {
    pub success: bool,
    pub message: String,
    pub error: Option<String>,
}

/// List pipeline repos visible to the authenticated user.
/// Super admins see all; other users see only their tenant's repos.
pub async fn list(pool: &PgPool, auth_user: &AuthUser) -> AppResult<Vec<PipelineRepo>> {
    let rows = if auth_user.is_super_admin() {
        sqlx::query_as::<_, PipelineRepo>("SELECT * FROM pipeline_repos ORDER BY name")
            .fetch_all(pool)
            .await?
    } else {
        sqlx::query_as::<_, PipelineRepo>(
            "SELECT * FROM pipeline_repos WHERE tenant_id = $1 ORDER BY name",
        )
        .bind(auth_user.tenant_id)
        .fetch_all(pool)
        .await?
    };
    Ok(rows)
}

/// Create a new pipeline repo. Validates name and repository non-empty.
pub async fn create(
    pool: &PgPool,
    auth_user: &AuthUser,
    req: CreatePipelineRepoRequest,
) -> AppResult<PipelineRepo> {
    require_non_empty(&req.name, "Name")?;
    require_non_empty(&req.repository, "Repository")?;

    let row = sqlx::query_as::<_, PipelineRepo>(
        r#"INSERT INTO pipeline_repos (repo_id, name, repository, token_secret_arn, description, enabled, tenant_id)
           VALUES ($1, $2, $3, $4, $5, $6, $7)
           RETURNING *"#,
    )
    .bind(&req.repo_id)
    .bind(&req.name)
    .bind(&req.repository)
    .bind(&req.token_secret_arn)
    .bind(&req.description)
    .bind(req.enabled)
    .bind(auth_user.tenant_id)
    .fetch_one(pool)
    .await?;

    Ok(row)
}

/// Update an existing pipeline repo. Non-admin users can only update repos
/// belonging to their own tenant.
pub async fn update(
    pool: &PgPool,
    auth_user: &AuthUser,
    id: Uuid,
    req: UpdatePipelineRepoRequest,
) -> AppResult<PipelineRepo> {
    if !auth_user.is_super_admin() {
        let existing =
            sqlx::query_as::<_, PipelineRepo>("SELECT * FROM pipeline_repos WHERE id = $1")
                .bind(id)
                .fetch_optional(pool)
                .await?
                .ok_or_else(|| AppError::NotFound("Pipeline repo not found".to_string()))?;
        if existing.tenant_id != auth_user.tenant_id {
            return Err(AppError::Forbidden("Access denied".to_string()));
        }
    }

    let row = sqlx::query_as::<_, PipelineRepo>(
        r#"UPDATE pipeline_repos SET
           name = COALESCE($2, name),
           repository = COALESCE($3, repository),
           token_secret_arn = COALESCE($4, token_secret_arn),
           description = COALESCE($5, description),
           enabled = COALESCE($6, enabled),
           updated_at = NOW()
           WHERE id = $1
           RETURNING *"#,
    )
    .bind(id)
    .bind(&req.name)
    .bind(&req.repository)
    .bind(&req.token_secret_arn)
    .bind(&req.description)
    .bind(req.enabled)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Pipeline repo not found".to_string()))?;

    Ok(row)
}

/// Delete a pipeline repo by ID. Non-admin users can only delete repos
/// belonging to their own tenant.
pub async fn delete(pool: &PgPool, auth_user: &AuthUser, id: Uuid) -> AppResult<()> {
    if !auth_user.is_super_admin() {
        let existing =
            sqlx::query_as::<_, PipelineRepo>("SELECT * FROM pipeline_repos WHERE id = $1")
                .bind(id)
                .fetch_optional(pool)
                .await?
                .ok_or_else(|| AppError::NotFound("Pipeline repo not found".to_string()))?;
        if existing.tenant_id != auth_user.tenant_id {
            return Err(AppError::Forbidden("Access denied".to_string()));
        }
    }

    let result = sqlx::query("DELETE FROM pipeline_repos WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Pipeline repo not found".to_string()));
    }

    Ok(())
}

/// Test git connection for an existing saved pipeline repo.
/// Fetches the repo, verifies tenant access, resolves secret token, runs git test.
pub async fn test_connection(
    pool: &PgPool,
    auth_user: &AuthUser,
    id: Uuid,
) -> AppResult<TestConnectionResult> {
    let repo = sqlx::query_as::<_, PipelineRepo>("SELECT * FROM pipeline_repos WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Pipeline repo not found".to_string()))?;

    if !auth_user.is_super_admin() && repo.tenant_id != auth_user.tenant_id {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }

    // Try to fetch stored token from Secrets Manager
    let token = if let Some(ref arn) = repo.token_secret_arn {
        if !arn.is_empty() {
            fetch_secret_value(arn).await.ok()
        } else {
            None
        }
    } else {
        None
    };

    Ok(run_git_test(&repo.repository, token.as_deref()).await)
}

/// Test git connection with inline URL + token (no saved repo needed).
pub async fn test_connection_inline(
    repository: &str,
    token: Option<&str>,
) -> AppResult<TestConnectionResult> {
    require_non_empty(repository, "Repository URL")?;
    Ok(run_git_test(repository, token).await)
}

/// Run `git ls-remote` against a URL (with optional token injected).
pub async fn run_git_test(repository: &str, token: Option<&str>) -> TestConnectionResult {
    let repo_url = match token {
        Some(t) if !t.is_empty() && repository.starts_with("https://") => {
            repository.replacen("https://", &format!("https://x-access-token:{}@", t), 1)
        }
        _ => repository.to_string(),
    };

    let output = tokio::process::Command::new("git")
        .args(["ls-remote", "--heads", &repo_url])
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .await;

    match output {
        Ok(out) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let branch_count = stdout.lines().count();
            TestConnectionResult {
                success: true,
                message: format!("Connected — {} branch(es) found", branch_count),
                error: None,
            }
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            TestConnectionResult {
                success: false,
                message: "Connection failed".to_string(),
                error: Some(sanitize_git_error(&stderr)),
            }
        }
        Err(e) => TestConnectionResult {
            success: false,
            message: "Failed to execute git command".to_string(),
            error: Some(e.to_string()),
        },
    }
}

/// Fetch a secret value from AWS Secrets Manager by ARN.
pub async fn fetch_secret_value(arn: &str) -> Result<String, String> {
    let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    let client = aws_sdk_secretsmanager::Client::new(&config);
    let result = client
        .get_secret_value()
        .secret_id(arn)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    result
        .secret_string()
        .map(|s| s.to_string())
        .ok_or_else(|| "Secret has no string value".to_string())
}

/// Remove tokens/credentials from git error messages.
pub fn sanitize_git_error(err: &str) -> String {
    let mut result = err.to_string();
    // Strip "://token@" patterns from URLs
    while let Some(start) = result.find("://") {
        let after = start + 3;
        if let Some(at_pos) = result[after..].find('@') {
            let at_abs = after + at_pos;
            result.replace_range(after..at_abs, "***");
        } else {
            break;
        }
    }
    result
}
