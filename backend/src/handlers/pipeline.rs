use axum::{
    Json,
    extract::{Path, State},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::AppState;
use crate::error::{AppError, AppResult};
use crate::middleware::auth::AuthUser;
use crate::models::pipeline::{CreatePipelineRepoRequest, PipelineRepo, UpdatePipelineRepoRequest};

#[derive(Debug, Serialize)]
pub struct TestConnectionResult {
    pub success: bool,
    pub message: String,
    pub error: Option<String>,
}

/// GET /api/pipeline/repos
pub async fn list(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
) -> AppResult<Json<Vec<PipelineRepo>>> {
    let rows = if auth_user.is_super_admin() {
        sqlx::query_as::<_, PipelineRepo>("SELECT * FROM pipeline_repos ORDER BY name")
            .fetch_all(&state.pool)
            .await?
    } else {
        sqlx::query_as::<_, PipelineRepo>("SELECT * FROM pipeline_repos WHERE tenant_id = $1 ORDER BY name")
            .bind(auth_user.tenant_id)
            .fetch_all(&state.pool)
            .await?
    };
    Ok(Json(rows))
}

/// POST /api/pipeline/repos
pub async fn create(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Json(req): Json<CreatePipelineRepoRequest>,
) -> AppResult<Json<PipelineRepo>> {
    if req.name.trim().is_empty() {
        return Err(AppError::BadRequest("Name is required".to_string()));
    }
    if req.repository.trim().is_empty() {
        return Err(AppError::BadRequest("Repository is required".to_string()));
    }

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
    .fetch_one(&state.pool)
    .await?;

    Ok(Json(row))
}

/// PUT /api/pipeline/repos/:id
pub async fn update(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdatePipelineRepoRequest>,
) -> AppResult<Json<PipelineRepo>> {
    if !auth_user.is_super_admin() {
        let existing = sqlx::query_as::<_, PipelineRepo>("SELECT * FROM pipeline_repos WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.pool)
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
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Pipeline repo not found".to_string()))?;

    Ok(Json(row))
}

/// DELETE /api/pipeline/repos/:id
pub async fn delete(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    if !auth_user.is_super_admin() {
        let existing = sqlx::query_as::<_, PipelineRepo>("SELECT * FROM pipeline_repos WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Pipeline repo not found".to_string()))?;
        if existing.tenant_id != auth_user.tenant_id {
            return Err(AppError::Forbidden("Access denied".to_string()));
        }
    }

    let result = sqlx::query("DELETE FROM pipeline_repos WHERE id = $1")
        .bind(id)
        .execute(&state.pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Pipeline repo not found".to_string()));
    }

    Ok(Json(serde_json::json!({"message": "Pipeline repo deleted"})))
}

/// Shared: run `git ls-remote` against a URL (with optional token injected).
async fn run_git_test(repository: &str, token: Option<&str>) -> TestConnectionResult {
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

/// POST /api/pipeline/repos/:id/test — test existing repo's git connection
pub async fn test_connection(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<TestConnectionResult>> {
    let repo = sqlx::query_as::<_, PipelineRepo>("SELECT * FROM pipeline_repos WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool)
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

    Ok(Json(run_git_test(&repo.repository, token.as_deref()).await))
}

#[derive(Debug, Deserialize)]
pub struct TestInlineRequest {
    pub repository: String,
    pub token: Option<String>,
}

/// POST /api/pipeline/repos/test — test connection with inline URL + token (no saved repo needed)
pub async fn test_connection_inline(
    _auth_user: axum::Extension<AuthUser>,
    State(_state): State<AppState>,
    Json(req): Json<TestInlineRequest>,
) -> AppResult<Json<TestConnectionResult>> {
    if req.repository.trim().is_empty() {
        return Err(AppError::BadRequest("Repository URL is required".to_string()));
    }
    Ok(Json(run_git_test(&req.repository, req.token.as_deref()).await))
}

/// Fetch a secret value from AWS Secrets Manager by ARN.
async fn fetch_secret_value(arn: &str) -> Result<String, String> {
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
fn sanitize_git_error(err: &str) -> String {
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
