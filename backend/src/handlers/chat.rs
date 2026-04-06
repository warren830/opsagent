use axum::{
    extract::{Path, State},
    response::sse::{Event, Sse},
    Json,
};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::path::PathBuf;
use std::pin::Pin;
use std::time::Duration;
use tokio_stream::Stream;

use crate::error::{AppError, AppResult};
use crate::middleware::auth::AuthUser;
use crate::services::claude::{ChatImageData, ClaudeService, StreamChunk};
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct ChatImage {
    /// Base64-encoded image data
    pub data: String,
    /// MIME type: image/png, image/jpeg, image/gif, image/webp
    pub media_type: String,
    /// Optional filename (used for display in frontend)
    #[serde(default)]
    #[allow(dead_code)]
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    pub message: String,
    /// Optional session_id to resume a conversation
    #[serde(default)]
    pub session_id: Option<String>,
    /// Optional system prompt override
    #[serde(default)]
    pub system_prompt: Option<String>,
    /// Optional attached images (base64)
    #[serde(default)]
    pub images: Vec<ChatImage>,
    /// Force new session (skip find_active_session)
    #[serde(default)]
    pub new_session: bool,
    /// Optional provider_id to select a specific model configuration
    #[serde(default)]
    pub provider_id: Option<uuid::Uuid>,
}

type SseEventStream = Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>>;

/// POST /api/chat — SSE streaming endpoint
/// Spawns Claude CLI and streams parsed chunks back as SSE events.
pub async fn stream(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Json(req): Json<ChatRequest>,
) -> Sse<axum::response::sse::KeepAliveStream<SseEventStream>> {
    let claude_bin = state.config.claude_bin.clone();

    // Per-user workspace: {claude_work_dir}/users/{user_id}/
    // Each user gets their own .claude/skills/ with symlinks to authorized skills only.
    let base_work_dir = PathBuf::from(&state.config.claude_work_dir);
    let user_work_dir = base_work_dir.join("users").join(auth_user.user_id.to_string());

    // Read model config from providers table, fallback to env config
    let (model, timeout, max_turns, provider_env_vars) =
        load_provider_config(&state, auth_user.tenant_id, req.provider_id).await;

    // Build per-user .claude/skills/ with symlinks to authorized skills
    setup_user_skill_symlinks(&state, &user_work_dir, auth_user.user_id, auth_user.tenant_id).await;

    let service = ClaudeService::new(claude_bin, user_work_dir.clone(), timeout, model, max_turns, state.pool.clone());

    // Try to find active session if not provided and not explicitly requesting a new one
    let session_id = if req.new_session {
        tracing::info!("New session requested, skipping find_active_session");
        None
    } else {
        match req.session_id {
            Some(sid) => Some(sid),
            None => {
                service
                    .find_active_session(auth_user.user_id, auth_user.tenant_id)
                    .await
            }
        }
    };

    // Build system prompt with account-level context
    let system_prompt = build_system_prompt(
        &state,
        &auth_user,
        &user_work_dir,
        req.system_prompt.as_deref(),
    )
    .await;

    // Convert images for Claude CLI stream-json input
    tracing::info!(
        "Chat request: message={}, images={}, session={:?}",
        req.message.len(),
        req.images.len(),
        session_id,
    );

    let images: Vec<ChatImageData> = req
        .images
        .iter()
        .map(|img| ChatImageData {
            data: img.data.clone(),
            media_type: img.media_type.clone(),
        })
        .collect();

    let user_id = auth_user.user_id;
    let tenant_id = auth_user.tenant_id;
    let pool = state.pool.clone();
    let message_text = req.message.clone();

    // Build env vars: provider config + AWS credentials from cloud accounts
    let mut all_env_vars = provider_env_vars;
    let aws_env_vars = build_aws_env_vars(&state, &auth_user).await;
    all_env_vars.extend(aws_env_vars);

    // Spawn Claude CLI process — skills are discovered via .claude/skills/ in user_work_dir
    let event_stream: SseEventStream = match service.run(
        &req.message,
        session_id.as_deref(),
        Some(&system_prompt),
        images,
        all_env_vars,
    ) {
        Ok(claude_stream) => {
            let sse_stream =
                tokio_stream::StreamExt::map(claude_stream, move |chunk| {
                    // Save session on init or done
                    match &chunk {
                        StreamChunk::Init {
                            session_id: Some(sid),
                        } => {
                            let pool = pool.clone();
                            let sid = sid.clone();
                            let msg = message_text.clone();
                            tokio::spawn(async move {
                                let title = if msg.len() > 50 {
                                    format!("{}...", &msg[..50])
                                } else {
                                    msg
                                };
                                let svc = ClaudeService::new(
                                    String::new(),
                                    PathBuf::from("."),
                                    Duration::from_secs(1),
                                    String::new(),
                                    25,
                                    pool,
                                );
                                let _ = svc
                                    .save_session(&sid, user_id, tenant_id, Some(&title))
                                    .await;
                            });
                        }
                        StreamChunk::Done {
                            session_id: Some(sid),
                            ..
                        } => {
                            let pool = pool.clone();
                            let sid = sid.clone();
                            tokio::spawn(async move {
                                let svc = ClaudeService::new(
                                    String::new(),
                                    PathBuf::from("."),
                                    Duration::from_secs(1),
                                    String::new(),
                                    25,
                                    pool,
                                );
                                let _ = svc
                                    .save_session(&sid, user_id, tenant_id, None)
                                    .await;
                            });
                        }
                        _ => {}
                    }

                    let data = serde_json::to_string(&chunk).unwrap_or_default();
                    Ok::<_, Infallible>(Event::default().data(data))
                });
            Box::pin(sse_stream)
        }
        Err(e) => {
            tracing::error!("Failed to spawn Claude CLI: {}", e);
            let error_stream = futures::stream::once(async move {
                let chunk = StreamChunk::Error {
                    message: format!("Failed to start Claude: {}", e),
                };
                let data = serde_json::to_string(&chunk).unwrap_or_default();
                Ok::<_, Infallible>(Event::default().data(data))
            });
            Box::pin(error_stream)
        }
    };

    Sse::new(event_stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("ping"),
    )
}

/// Build AWS credential environment variables from the tenant's primary cloud account.
/// Returns env vars to inject into the Claude CLI subprocess.
async fn build_aws_env_vars(
    state: &AppState,
    auth_user: &AuthUser,
) -> Vec<(String, String)> {
    let mut env_vars = Vec::new();

    let account_ids = crate::handlers::account_access::get_accessible_account_ids(
        &state.pool, auth_user,
    ).await;

    if account_ids.is_empty() {
        return env_vars;
    }

    // Find primary AWS account (first non-mock AWS account the user can access)
    let account = sqlx::query_as::<_, (Option<String>, Option<String>, Vec<String>)>(
        r#"SELECT role_arn, profile, regions FROM cloud_accounts
           WHERE provider = 'aws' AND is_mock = false AND id = ANY($1)
           ORDER BY created_at ASC LIMIT 1"#,
    )
    .bind(&account_ids)
    .fetch_optional(&state.pool)
    .await
    .ok()
    .flatten();

    if let Some((role_arn, profile, regions)) = account {
        if let Some(arn) = role_arn {
            if !arn.is_empty() {
                env_vars.push(("AWS_ROLE_ARN".to_string(), arn));
                env_vars.push(("AWS_ROLE_SESSION_NAME".to_string(), "openops-chat".to_string()));
            }
        }
        if let Some(prof) = profile {
            if !prof.is_empty() {
                env_vars.push(("AWS_PROFILE".to_string(), prof));
            }
        }
        if let Some(first_region) = regions.first() {
            env_vars.push(("AWS_DEFAULT_REGION".to_string(), first_region.clone()));
        }
    }

    if !env_vars.is_empty() {
        tracing::info!("Injecting AWS env vars: {:?}", env_vars.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>());
    }

    env_vars
}

/// Build system prompt with account-level context (glossary, knowledge, accounts)
async fn build_system_prompt(
    state: &AppState,
    auth_user: &AuthUser,
    user_work_dir: &PathBuf,
    custom: Option<&str>,
) -> String {
    let account_ids = crate::handlers::account_access::get_accessible_account_ids(
        &state.pool, auth_user,
    ).await;
    let workspace_path = std::fs::canonicalize(user_work_dir)
        .unwrap_or_else(|_| user_work_dir.clone());

    let mut parts = vec![
        "You are OpenOps AI, a multi-cloud infrastructure operations assistant.".to_string(),
        "You help users manage AWS, Alicloud, and Azure cloud resources.".to_string(),
        "Answer in the user's language. Be concise and actionable.".to_string(),
        format!("\n## CRITICAL ENVIRONMENT RULES (override any skill instructions)"),
        "You are running inside the OpenOps platform. The following rules OVERRIDE any instructions from skills or SKILL.md files:".to_string(),
        format!("1. WORKSPACE: All output files MUST be saved to: {}", workspace_path.display()),
        "2. CREDENTIALS: AWS credentials are ALREADY configured via environment variables and IAM roles. NEVER ask the user for AWS Profile, AK/SK, credentials, or authentication method. Always use `--auth default` or equivalent default credential chain.".to_string(),
        "3. ACCOUNTS: The tenant's cloud accounts are listed below. Use them directly. NEVER ask the user to provide account IDs.".to_string(),
        "4. AUTO-FILL THESE (do NOT ask): auth method (always default credentials), output path (always workspace).".to_string(),
        format!("   - Output path: {}", workspace_path.display()),
        "5. MUST ASK USER for scope selection: When a skill requires the user to choose regions, months, time ranges, or other scan scope parameters, you MUST ask and wait for the user's response before executing. Do NOT auto-select all or assume defaults for scope.".to_string(),
    ];

    // Inject glossary terms (filtered by accessible accounts)
    if let Ok(terms) = sqlx::query_as::<_, (String, Option<String>, Option<String>)>(
        "SELECT term, full_name, description FROM glossary WHERE account_id = ANY($1) OR account_id IS NULL LIMIT 50",
    )
    .bind(&account_ids)
    .fetch_all(&state.pool)
    .await
    {
        if !terms.is_empty() {
            parts.push("\n## Internal Glossary".to_string());
            for (term, full_name, desc) in terms {
                let full = full_name.unwrap_or_default();
                let d = desc.unwrap_or_default();
                parts.push(format!("- **{}** ({}): {}", term, full, d));
            }
        }
    }

    // Inject knowledge base items (filtered by accessible accounts)
    if let Ok(docs) = sqlx::query_as::<_, (String, Option<String>)>(
        "SELECT filename, content FROM knowledge_files WHERE (account_id = ANY($1) OR account_id IS NULL) AND content IS NOT NULL LIMIT 20",
    )
    .bind(&account_ids)
    .fetch_all(&state.pool)
    .await
    {
        if !docs.is_empty() {
            parts.push("\n## Knowledge Base".to_string());
            for (title, content) in docs {
                let c = content.unwrap_or_default();
                let truncated = if c.len() > 500 { &c[..500] } else { &c };
                parts.push(format!("### {}\n{}", title, truncated));
            }
        }
    }

    // Inject cloud accounts info (with regions array, filtered by accessible accounts)
    if let Ok(accounts) = sqlx::query_as::<_, (String, String, Option<String>, Option<String>, Vec<String>)>(
        "SELECT provider, name, account_id, role_arn, regions FROM cloud_accounts WHERE id = ANY($1) AND is_mock = false LIMIT 20",
    )
    .bind(&account_ids)
    .fetch_all(&state.pool)
    .await
    {
        if !accounts.is_empty() {
            parts.push("\n## Available Cloud Accounts".to_string());
            for (provider, name, account_id, role_arn, regions) in &accounts {
                let aid = account_id.as_deref().unwrap_or("-");
                let regions_str = if regions.is_empty() { "ALL (no restriction)".to_string() } else { regions.join(", ") };
                let role_info = role_arn.as_deref().map(|r| format!(", Role: {}", r)).unwrap_or_default();
                parts.push(format!("- {} ({}) — Account: {}, Regions: [{}]{}", name, provider, aid, regions_str, role_info));
            }
        }
    }

    // Append custom system prompt
    if let Some(custom) = custom {
        parts.push(format!("\n## Additional Context\n{}", custom));
    }

    parts.join("\n")
}

/// Build per-user `.claude/skills/` directory with symlinks to authorized skills only.
/// This ensures Claude CLI's native skill discovery (`/skill-name`) only sees
/// skills the user has permission to access (private + tenant-public).
async fn setup_user_skill_symlinks(
    state: &AppState,
    user_work_dir: &PathBuf,
    user_id: uuid::Uuid,
    tenant_id: Option<uuid::Uuid>,
) {
    let skills_link_dir = user_work_dir.join(".claude").join("skills");

    // Create the directory structure
    if let Err(e) = tokio::fs::create_dir_all(&skills_link_dir).await {
        tracing::warn!("Failed to create user skills dir {:?}: {}", skills_link_dir, e);
        return;
    }

    // Query authorized skills (same WHERE clause as list handler)
    // Use IS NOT DISTINCT FROM for tenant_id because both sides can be NULL
    let authorized: Vec<(String, Option<String>)> = sqlx::query_as(
        r#"SELECT name, repo_path FROM skills
           WHERE enabled = true AND repo_path IS NOT NULL
             AND ((user_id = $1) OR (user_id IS NULL AND tenant_id IS NOT DISTINCT FROM $2))"#,
    )
    .bind(user_id)
    .bind(tenant_id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    // Collect authorized skill dir names
    let authorized_names: std::collections::HashSet<String> = authorized
        .iter()
        .filter_map(|(_, rp)| {
            rp.as_ref().and_then(|p| {
                std::path::Path::new(p)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|s| s.to_string())
            })
        })
        .collect();

    // Remove stale symlinks (skills user no longer has access to)
    if let Ok(mut entries) = tokio::fs::read_dir(&skills_link_dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let name = entry.file_name().to_string_lossy().to_string();
            if !authorized_names.contains(&name) {
                let _ = tokio::fs::remove_file(entry.path()).await;
                let _ = tokio::fs::remove_dir_all(entry.path()).await;
            }
        }
    }

    // Create/update symlinks for authorized skills
    for (_name, repo_path) in &authorized {
        if let Some(rp) = repo_path {
            let src = std::path::Path::new(rp);
            if let Some(dir_name) = src.file_name() {
                let link = skills_link_dir.join(dir_name);
                // Skip if symlink already points to the right place
                if let Ok(target) = tokio::fs::read_link(&link).await {
                    if target == src {
                        continue;
                    }
                    // Stale symlink, remove
                    let _ = tokio::fs::remove_file(&link).await;
                }
                if src.exists() {
                    if let Err(e) = tokio::fs::symlink(src, &link).await {
                        tracing::warn!("Failed to symlink skill {:?} -> {:?}: {}", link, src, e);
                    }
                }
            }
        }
    }
}

/// GET /api/chat/sessions — list user's chat sessions
pub async fn list_sessions(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
) -> crate::error::AppResult<Json<Vec<ChatSession>>> {
    let sessions = sqlx::query_as::<_, ChatSession>(
        r#"SELECT id, claude_session_id, title, last_message, is_active, created_at, last_active_at
           FROM claude_sessions
           WHERE user_id = $1
           AND ($2::UUID IS NULL OR tenant_id = $2)
           ORDER BY last_active_at DESC
           LIMIT 50"#,
    )
    .bind(auth_user.user_id)
    .bind(auth_user.tenant_id)
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(sessions))
}

/// Build provider-specific environment variables for Claude CLI
fn build_provider_env_vars(
    provider_type: &str,
    config: &serde_json::Value,
) -> Vec<(String, String)> {
    let mut env = Vec::new();
    match provider_type {
        "bedrock" => {
            env.push(("CLAUDE_CODE_USE_BEDROCK".to_string(), "1".to_string()));
            if let Some(r) = config.get("region").and_then(|v| v.as_str()) {
                env.push(("AWS_REGION".to_string(), r.to_string()));
            }
        }
        "gateway" => {
            if let Some(u) = config.get("base_url").and_then(|v| v.as_str()) {
                if !u.is_empty() {
                    env.push(("ANTHROPIC_BASE_URL".to_string(), u.to_string()));
                }
            }
            if let Some(k) = config.get("api_key").and_then(|v| v.as_str()) {
                if !k.is_empty() {
                    env.push(("ANTHROPIC_API_KEY".to_string(), k.to_string()));
                }
            }
        }
        _ => {}
    }
    env
}

/// Load model + timeout + max_turns + provider env vars from providers table, fallback to env config.
/// If provider_id is given, use that specific provider; otherwise use the tenant's default.
async fn load_provider_config(
    state: &AppState,
    tenant_id: Option<uuid::Uuid>,
    provider_id: Option<uuid::Uuid>,
) -> (String, Duration, u32, Vec<(String, String)>) {
    let row = if let Some(pid) = provider_id {
        // Use specific provider
        sqlx::query_as::<_, (String, serde_json::Value)>(
            "SELECT provider_type, config FROM providers WHERE id = $1",
        )
        .bind(pid)
        .fetch_optional(&state.pool)
        .await
        .ok()
        .flatten()
    } else {
        // Tenant default → global default → fallback
        sqlx::query_as::<_, (String, serde_json::Value)>(
            r#"SELECT provider_type, config FROM providers
               WHERE (tenant_id = $1 AND is_default = true)
                  OR (tenant_id IS NULL AND is_default = true)
               ORDER BY CASE WHEN tenant_id = $1 THEN 0 ELSE 1 END
               LIMIT 1"#,
        )
        .bind(tenant_id)
        .fetch_optional(&state.pool)
        .await
        .ok()
        .flatten()
    };

    if let Some((provider_type, config)) = row {
        let model = config
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or(&state.config.claude_model)
            .to_string();
        let timeout_ms = config
            .get("timeout")
            .and_then(|v| v.as_u64())
            .unwrap_or(state.config.claude_timeout_ms);
        let max_turns = config
            .get("max_turns")
            .and_then(|v| v.as_u64())
            .unwrap_or(25) as u32;
        let provider_envs = build_provider_env_vars(&provider_type, &config);

        (model, Duration::from_millis(timeout_ms), max_turns, provider_envs)
    } else {
        (
            state.config.claude_model.clone(),
            Duration::from_millis(state.config.claude_timeout_ms),
            25,
            Vec::new(),
        )
    }
}

#[derive(Debug, serde::Serialize, sqlx::FromRow)]
pub struct ChatSession {
    pub id: uuid::Uuid,
    pub claude_session_id: String,
    pub title: Option<String>,
    pub last_message: Option<String>,
    pub is_active: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_active_at: chrono::DateTime<chrono::Utc>,
}

// ─── Workspace ──────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct WorkspaceFile {
    pub name: String,
    pub size: u64,
    pub modified: String,
    pub is_dir: bool,
}

/// GET /api/chat/workspace — list files in user's workspace (recursive)
pub async fn workspace_list(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
) -> AppResult<Json<Vec<WorkspaceFile>>> {
    let base = PathBuf::from(&state.config.claude_work_dir);
    let user_dir = base.join("users").join(auth_user.user_id.to_string());

    let mut files = Vec::new();
    collect_workspace_files(&user_dir, &user_dir, &mut files).await;
    files.sort_by(|a, b| b.modified.cmp(&a.modified));
    Ok(Json(files))
}

/// Recursively collect files from workspace, using relative paths from root
async fn collect_workspace_files(
    root: &std::path::Path,
    dir: &std::path::Path,
    files: &mut Vec<WorkspaceFile>,
) {
    let Ok(mut entries) = tokio::fs::read_dir(dir).await else { return };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name().to_string_lossy().to_string();
        // Skip hidden dirs (.claude, .git, etc.)
        if name.starts_with('.') { continue; }
        let path = entry.path();
        if let Ok(meta) = entry.metadata().await {
            let rel_path = path.strip_prefix(root)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or(name.clone());
            if meta.is_dir() {
                Box::pin(collect_workspace_files(root, &path, files)).await;
            } else {
                files.push(WorkspaceFile {
                    name: rel_path,
                    size: meta.len(),
                    modified: chrono::DateTime::<chrono::Utc>::from(
                        meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH)
                    )
                    .format("%Y-%m-%d %H:%M")
                    .to_string(),
                    is_dir: false,
                });
            }
        }
    }
}

/// GET /api/chat/workspace/*filepath — download a file from workspace
pub async fn workspace_download(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(filename): Path<String>,
) -> Result<axum::response::Response, AppError> {
    use axum::response::IntoResponse;

    // Sanitize — no path traversal
    if filename.contains("..") || filename.contains('\\') {
        return Err(AppError::BadRequest("Invalid filename".to_string()));
    }

    let base = PathBuf::from(&state.config.claude_work_dir);
    let user_dir = base.join("users").join(auth_user.user_id.to_string());
    let file_path = user_dir.join(&filename);

    // Ensure resolved path is still under user dir (prevent symlink escape)
    if let (Ok(resolved), Ok(user_resolved)) = (file_path.canonicalize(), user_dir.canonicalize()) {
        if !resolved.starts_with(&user_resolved) {
            return Err(AppError::BadRequest("Invalid path".to_string()));
        }
    }

    if !file_path.exists() || file_path.is_dir() {
        return Err(AppError::NotFound("File not found".to_string()));
    }

    let bytes = tokio::fs::read(&file_path).await
        .map_err(|e| AppError::Internal(format!("Failed to read file: {}", e)))?;

    let content_type = if filename.ends_with(".xlsx") {
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
    } else if filename.ends_with(".csv") {
        "text/csv"
    } else if filename.ends_with(".json") {
        "application/json"
    } else if filename.ends_with(".pdf") {
        "application/pdf"
    } else {
        "application/octet-stream"
    };

    Ok((
        [
            (http::header::CONTENT_TYPE, content_type),
            (http::header::CONTENT_DISPOSITION, &format!("attachment; filename=\"{}\"", filename.split('/').last().unwrap_or(&filename))),
        ],
        bytes,
    ).into_response())
}

/// DELETE /api/chat/workspace/*filepath — delete a file from workspace
pub async fn workspace_delete(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(filename): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    // Sanitize — no path traversal
    if filename.contains("..") || filename.contains('\\') {
        return Err(AppError::BadRequest("Invalid filename".to_string()));
    }

    let base = PathBuf::from(&state.config.claude_work_dir);
    let user_dir = base.join("users").join(auth_user.user_id.to_string());
    let file_path = user_dir.join(&filename);

    // Ensure resolved path is still under user dir
    if let (Ok(resolved), Ok(user_resolved)) = (file_path.canonicalize(), user_dir.canonicalize()) {
        if !resolved.starts_with(&user_resolved) {
            return Err(AppError::BadRequest("Invalid path".to_string()));
        }
    }

    if !file_path.exists() {
        return Err(AppError::NotFound("File not found".to_string()));
    }

    if file_path.is_dir() {
        tokio::fs::remove_dir_all(&file_path).await
            .map_err(|e| AppError::Internal(format!("Failed to delete directory: {}", e)))?;
    } else {
        tokio::fs::remove_file(&file_path).await
            .map_err(|e| AppError::Internal(format!("Failed to delete file: {}", e)))?;
    }

    // Clean up empty parent dirs (up to user_dir)
    let mut current = file_path.parent().map(|p| p.to_path_buf());
    let user_dir_resolved = user_dir.canonicalize().unwrap_or(user_dir.clone());
    while let Some(p) = current {
        let p_resolved = p.canonicalize().unwrap_or(p.clone());
        if p_resolved == user_dir_resolved { break; }
        // Try to remove — only succeeds if empty
        if tokio::fs::remove_dir(&p).await.is_err() { break; }
        current = p.parent().map(|pp| pp.to_path_buf());
    }

    Ok(Json(serde_json::json!({"message": "Deleted"})))
}
