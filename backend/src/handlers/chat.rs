use axum::{
    Json,
    extract::{Path, State},
    response::sse::{Event, Sse},
};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::path::PathBuf;
use std::pin::Pin;
use std::time::Duration;
use tokio_stream::Stream;

use crate::AppState;
use crate::error::{AppError, AppResult};
use crate::middleware::auth::{AuthUser, Claims};
use crate::services::claude::{ChatImageData, ClaudeService, StreamChunk};

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
    /// Optional MCP server IDs to include (None = all enabled)
    #[serde(default)]
    pub mcp_server_ids: Option<Vec<uuid::Uuid>>,
    /// Disabled MCP tools in "serverId:toolName" format → mapped to mcp__serverName__toolName
    #[serde(default)]
    pub disabled_mcp_tools: Option<Vec<String>>,
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
    let provider = load_provider_config(&state, auth_user.tenant_id, req.provider_id).await;

    tracing::info!(
        "Provider config: model={}, provider_id={:?}, permission={}, disallowed={:?}, allowed={:?}, env_keys={:?}",
        provider.model,
        req.provider_id,
        provider.permission_mode,
        provider.disallowed_tools,
        provider.allowed_tools,
        provider.env_vars.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>()
    );

    // Build per-user .claude/skills/ with symlinks to authorized skills
    setup_user_skill_symlinks(&state, &user_work_dir, auth_user.user_id, auth_user.tenant_id).await;

    // Write CLAUDE.md to user workspace — Claude CLI natively loads this as project instructions.
    // This is far more effective than --system-prompt for controlling agent behavior.
    write_user_claude_md(&state, &auth_user, &user_work_dir).await;

    let service = ClaudeService::new(
        claude_bin,
        user_work_dir.clone(),
        provider.timeout,
        provider.model.clone(),
        provider.max_turns,
        state.pool.clone(),
    );

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
    let system_prompt = build_system_prompt(&state, &auth_user, &user_work_dir, req.system_prompt.as_deref()).await;

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
    let mut all_env_vars = provider.env_vars;
    let aws_env_vars = build_aws_env_vars(&state, &auth_user).await;
    all_env_vars.extend(aws_env_vars);

    // Generate short-lived API token for agent to call OpenOps APIs
    if let Some(token) = generate_agent_token(&auth_user, &state.config.jwt_secret) {
        all_env_vars.push(("OPENOPS_API_TOKEN".to_string(), token));
        all_env_vars.push((
            "OPENOPS_API_BASE".to_string(),
            format!("http://localhost:{}", state.config.backend_port),
        ));
    }

    // Build MCP config from user's enabled MCP servers (writes to file in user_work_dir)
    let api_token_for_mcp = all_env_vars
        .iter()
        .find(|(k, _)| k == "OPENOPS_API_TOKEN")
        .map(|(_, v)| v.clone());
    let (mcp_config, mcp_server_names) = build_mcp_config(
        &state,
        &auth_user,
        &user_work_dir,
        req.mcp_server_ids.as_deref(),
        api_token_for_mcp.as_deref(),
    )
    .await;

    // Auto-allow all MCP tools so Claude CLI doesn't prompt for permission
    let mut allowed_tools = provider.allowed_tools.clone();
    for name in &mcp_server_names {
        allowed_tools.push(format!("mcp__{}__*", name));
    }

    // Build final disallowed tools list: provider defaults + disabled MCP tools
    let mut disallowed_tools = provider.disallowed_tools.clone();
    if let Some(disabled) = &req.disabled_mcp_tools {
        // Map "serverId:toolName" → "mcp__serverName__toolName" (Claude CLI format)
        for entry in disabled {
            if let Some((server_id_str, tool_name)) = entry.split_once(':') {
                // Look up server name from MCP servers loaded earlier
                if let Ok(sid) = uuid::Uuid::parse_str(server_id_str)
                    && let Ok(name) = sqlx::query_scalar::<_, String>("SELECT name FROM mcp_servers WHERE id = $1")
                        .bind(sid)
                        .fetch_one(&state.pool)
                        .await
                {
                    disallowed_tools.push(format!("mcp__{}__{}", name, tool_name));
                }
            }
        }
    }

    // Spawn Claude CLI process — skills are discovered via .claude/skills/ in user_work_dir
    let event_stream: SseEventStream = match service.run(
        &req.message,
        session_id.as_deref(),
        Some(&system_prompt),
        images,
        all_env_vars,
        &provider.permission_mode,
        &disallowed_tools,
        &allowed_tools,
        mcp_config.as_deref(),
    ) {
        Ok(claude_stream) => {
            let sse_stream = tokio_stream::StreamExt::map(claude_stream, move |chunk| {
                // Save session on init or done
                match &chunk {
                    StreamChunk::Init { session_id: Some(sid) } => {
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
                            if let Err(e) = svc.save_session(&sid, user_id, tenant_id, Some(&title)).await {
                                tracing::error!("Failed to save session (init): {}", e);
                            }
                        });
                    }
                    StreamChunk::Done {
                        session_id: Some(sid), ..
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
                            if let Err(e) = svc.save_session(&sid, user_id, tenant_id, None).await {
                                tracing::error!("Failed to save session (done): {}", e);
                            }
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

/// Generate a short-lived JWT for the agent to call OpenOps APIs
fn generate_agent_token(auth_user: &AuthUser, jwt_secret: &str) -> Option<String> {
    let now = chrono::Utc::now().timestamp() as usize;
    let claims = Claims {
        sub: auth_user.user_id,
        role: auth_user.role.clone(),
        tenant_id: auth_user.tenant_id,
        username: auth_user.username.clone(),
        token_type: "access".to_string(),
        iat: now,
        exp: now + 7200, // 2 hours — covers long-running agent sessions
    };
    jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(jwt_secret.as_bytes()),
    )
    .ok()
}

/// Build AWS credential environment variables from the tenant's primary cloud account.
/// Returns env vars to inject into the Claude CLI subprocess.
async fn build_aws_env_vars(state: &AppState, auth_user: &AuthUser) -> Vec<(String, String)> {
    let mut env_vars = Vec::new();

    let account_ids = crate::handlers::account_access::get_accessible_account_ids(&state.pool, auth_user).await;

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
        if let Some(arn) = role_arn
            && !arn.is_empty()
        {
            env_vars.push(("AWS_ROLE_ARN".to_string(), arn));
            env_vars.push(("AWS_ROLE_SESSION_NAME".to_string(), "openops-chat".to_string()));
        }
        if let Some(prof) = profile
            && !prof.is_empty()
        {
            env_vars.push(("AWS_PROFILE".to_string(), prof));
        }
        if let Some(first_region) = regions.first() {
            env_vars.push(("AWS_DEFAULT_REGION".to_string(), first_region.clone()));
        }
    }

    if !env_vars.is_empty() {
        tracing::info!(
            "Injecting AWS env vars: {:?}",
            env_vars.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>()
        );
    }

    env_vars
}

/// Build MCP config file for Claude CLI --mcp-config flag.
/// Queries enabled MCP servers, writes JSON to a temp file in user_work_dir,
/// and returns (file_path, server_names). Claude CLI expects a file path, not a JSON string.
/// When `server_ids` is Some, only include those specific servers.
async fn build_mcp_config(
    state: &AppState,
    auth_user: &AuthUser,
    user_work_dir: &std::path::Path,
    server_ids: Option<&[uuid::Uuid]>,
    api_token: Option<&str>,
) -> (Option<String>, Vec<String>) {
    let all_servers = sqlx::query_as::<_, crate::models::mcp::McpServer>(
        r#"SELECT * FROM mcp_servers
           WHERE enabled = true
           AND ((user_id = $1) OR (user_id IS NULL AND tenant_id IS NOT DISTINCT FROM $2))
           ORDER BY name"#,
    )
    .bind(auth_user.user_id)
    .bind(auth_user.tenant_id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    // Filter by requested IDs if provided
    let servers: Vec<_> = match server_ids {
        Some(ids) if !ids.is_empty() => all_servers.into_iter().filter(|s| ids.contains(&s.id)).collect(),
        _ => all_servers,
    };

    if servers.is_empty() {
        return (None, Vec::new());
    }

    let server_names: Vec<String> = servers.iter().map(|s| s.name.clone()).collect();
    let mut mcp_servers = serde_json::Map::new();

    for srv in &servers {
        let entry = match srv.transport_type.as_str() {
            "sse" | "http" => {
                let mut obj = serde_json::Map::new();
                obj.insert("type".to_string(), serde_json::json!(srv.transport_type));
                if let Some(url) = &srv.url {
                    obj.insert("url".to_string(), serde_json::json!(url));
                }
                // For openops-* built-in servers, auto-inject Authorization header
                let mut merged_headers = if srv.headers != serde_json::json!({}) {
                    srv.headers.clone()
                } else {
                    serde_json::json!({})
                };
                if srv.name.starts_with("openops-")
                    && let Some(token) = api_token
                    && let Some(obj_map) = merged_headers.as_object_mut()
                {
                    obj_map.insert(
                        "Authorization".to_string(),
                        serde_json::json!(format!("Bearer {}", token)),
                    );
                }
                if merged_headers != serde_json::json!({}) {
                    obj.insert("headers".to_string(), merged_headers);
                }
                if srv.env != serde_json::json!({}) {
                    obj.insert("env".to_string(), srv.env.clone());
                }
                serde_json::Value::Object(obj)
            }
            _ => {
                // stdio
                let mut obj = serde_json::Map::new();
                obj.insert("command".to_string(), serde_json::json!(srv.command));
                if srv.args != serde_json::json!([]) {
                    obj.insert("args".to_string(), srv.args.clone());
                }
                if srv.env != serde_json::json!({}) {
                    obj.insert("env".to_string(), srv.env.clone());
                }
                serde_json::Value::Object(obj)
            }
        };
        mcp_servers.insert(srv.name.clone(), entry);
    }

    let config = serde_json::json!({ "mcpServers": mcp_servers });

    tracing::info!(
        "MCP config: {} server(s): {:?}",
        servers.len(),
        servers.iter().map(|s| &s.name).collect::<Vec<_>>()
    );

    // Write config to a file inside user_work_dir (works in local dev + cloud EKS pods)
    let config_path = user_work_dir.join(".mcp.json");
    if let Err(e) = tokio::fs::create_dir_all(user_work_dir).await {
        tracing::error!("Failed to create user work dir for MCP config: {}", e);
        return (None, server_names);
    }
    match tokio::fs::write(&config_path, serde_json::to_string_pretty(&config).unwrap_or_default()).await {
        Ok(_) => {
            tracing::info!("MCP config written to {}", config_path.display());
            // Return just the filename — Claude CLI runs with current_dir=user_work_dir
            (Some(".mcp.json".to_string()), server_names)
        }
        Err(e) => {
            tracing::error!("Failed to write MCP config file: {}", e);
            (None, server_names)
        }
    }
}

/// Build system prompt — lightweight since CLAUDE.md handles the heavy lifting.
/// System prompt is for basic role identity; CLAUDE.md (loaded natively by Claude CLI)
/// handles all detailed instructions, API endpoints, glossary, and accounts.
async fn build_system_prompt(
    _state: &AppState,
    _auth_user: &AuthUser,
    _user_work_dir: &std::path::Path,
    custom: Option<&str>,
) -> String {
    let mut parts = vec![
        "You are OpenOps AI, a multi-cloud infrastructure operations assistant.".to_string(),
        "Answer in the user's language. Be concise and actionable.".to_string(),
        "Follow all instructions in CLAUDE.md carefully.".to_string(),
    ];

    if let Some(custom) = custom {
        parts.push(format!("\n{}", custom));
    }

    parts.join("\n")
}

/// Write CLAUDE.md into the user's workspace directory.
/// Claude CLI natively loads CLAUDE.md as project-level instructions with high priority.
/// This is far more effective than --system-prompt for controlling agent behavior.
async fn write_user_claude_md(state: &AppState, auth_user: &AuthUser, user_work_dir: &std::path::Path) {
    let account_ids = crate::handlers::account_access::get_accessible_account_ids(&state.pool, auth_user).await;
    let workspace_path = std::fs::canonicalize(user_work_dir).unwrap_or_else(|_| user_work_dir.to_path_buf());

    let mut lines = Vec::new();

    lines.push("# OpenOps Agent Instructions".to_string());
    lines.push(String::new());
    lines.push("You are OpenOps AI, a multi-cloud infrastructure operations assistant.".to_string());
    lines.push("Answer in the user's language. Be concise and actionable.".to_string());
    lines.push(String::new());

    // Environment rules
    lines.push("## Environment Rules".to_string());
    lines.push(String::new());
    lines.push(format!(
        "- **Workspace**: All output files MUST be saved to `{}`",
        workspace_path.display()
    ));
    lines.push("- **Credentials**: AWS credentials are pre-configured via environment variables. NEVER ask the user for credentials.".to_string());
    lines.push(
        "- **Scope**: When a task requires choosing regions, months, time ranges — always ASK the user first."
            .to_string(),
    );
    lines.push(String::new());

    // MCP / RAG image handling
    lines.push("## MCP & RAG Image Handling".to_string());
    lines.push(String::new());
    lines.push("When RAG tools (e.g. rag_tool) return content containing images in markdown format like `![IMAGE: description](https://...)`, you MUST:".to_string());
    lines.push(
        "1. **Include the original image URL** as-is in your response using markdown: `![description](url)`"
            .to_string(),
    );
    lines.push("2. **NEVER recreate diagrams** as mermaid/ASCII/text when the original image is available".to_string());
    lines.push("3. The frontend renders markdown images natively — just pass the URL through".to_string());
    lines.push(String::new());

    // Knowledge API — the critical part
    lines.push("## How to Answer Knowledge Questions".to_string());
    lines.push(String::new());
    lines.push("When the user asks about **internal terminology, glossary, abbreviations, runbooks, knowledge base entries, cloud accounts, or security findings**, you MUST query the OpenOps API.".to_string());
    lines.push(String::new());
    lines.push("The knowledge is stored in a database, NOT in local files. Do NOT search or read local files for this information.".to_string());
    lines.push(String::new());
    lines.push("Use these commands (env vars are pre-set):".to_string());
    lines.push(String::new());
    lines.push("All APIs use the same auth header: `Authorization: Bearer $OPENOPS_API_TOKEN`".to_string());
    lines.push("Base URL is `$OPENOPS_API_BASE`. Both env vars are pre-set.".to_string());
    lines.push(String::new());
    lines.push("### Discovery & Assets".to_string());
    lines.push("```bash".to_string());
    lines.push("# Cloud accounts (provider, account_id, regions, role_arn)".to_string());
    lines
        .push("curl -s -H \"Authorization: Bearer $OPENOPS_API_TOKEN\" \"$OPENOPS_API_BASE/api/accounts\"".to_string());
    lines.push("# Kubernetes clusters (name, cloud, region, status, endpoint)".to_string());
    lines
        .push("curl -s -H \"Authorization: Bearer $OPENOPS_API_TOKEN\" \"$OPENOPS_API_BASE/api/clusters\"".to_string());
    lines.push("# Service topology (real-time Ingress→Service→Deployment/Rollout graph)".to_string());
    lines
        .push("curl -s -H \"Authorization: Bearer $OPENOPS_API_TOKEN\" \"$OPENOPS_API_BASE/api/topology\"".to_string());
    lines.push("# Security resources & findings".to_string());
    lines.push(
        "curl -s -H \"Authorization: Bearer $OPENOPS_API_TOKEN\" \"$OPENOPS_API_BASE/api/resources\"".to_string(),
    );
    lines.push(
        "curl -s -H \"Authorization: Bearer $OPENOPS_API_TOKEN\" \"$OPENOPS_API_BASE/api/resources/dashboard\""
            .to_string(),
    );
    lines.push("```".to_string());
    lines.push(String::new());
    lines.push("### Deployments (Argo Rollouts)".to_string());
    lines.push("```bash".to_string());
    lines.push("# List rollouts on a cluster".to_string());
    lines.push("curl -s -H \"Authorization: Bearer $OPENOPS_API_TOKEN\" \"$OPENOPS_API_BASE/api/clusters/{cluster_id}/rollouts\"".to_string());
    lines.push("# Get rollout detail (canary steps, containers)".to_string());
    lines.push("curl -s -H \"Authorization: Bearer $OPENOPS_API_TOKEN\" \"$OPENOPS_API_BASE/api/clusters/{cluster_id}/rollouts/{ns}/{name}\"".to_string());
    lines.push("# Promote rollout (step or full)".to_string());
    lines.push("curl -s -X POST -H \"Authorization: Bearer $OPENOPS_API_TOKEN\" -H 'Content-Type: application/json' -d '{\"full\":false}' \"$OPENOPS_API_BASE/api/clusters/{cluster_id}/rollouts/{ns}/{name}/promote\"".to_string());
    lines.push("# Rollback rollout".to_string());
    lines.push("curl -s -X POST -H \"Authorization: Bearer $OPENOPS_API_TOKEN\" \"$OPENOPS_API_BASE/api/clusters/{cluster_id}/rollouts/{ns}/{name}/rollback\"".to_string());
    lines.push("# Change strategy (canary/blueGreen/rollingUpdate)".to_string());
    lines.push("curl -s -X POST -H \"Authorization: Bearer $OPENOPS_API_TOKEN\" -H 'Content-Type: application/json' -d '{\"strategy\":\"canary\",\"canarySteps\":[{\"setWeight\":20},{\"pause\":{}},{\"setWeight\":50},{\"pause\":{\"duration\":\"60s\"}}]}' \"$OPENOPS_API_BASE/api/clusters/{cluster_id}/rollouts/{ns}/{name}/strategy\"".to_string());
    lines.push("# Analysis runs for a rollout".to_string());
    lines.push("curl -s -H \"Authorization: Bearer $OPENOPS_API_TOKEN\" \"$OPENOPS_API_BASE/api/clusters/{cluster_id}/rollouts/{ns}/{name}/analysis\"".to_string());
    lines.push("# Deployment history (audit log of promote/rollback/strategy changes)".to_string());
    lines.push(
        "curl -s -H \"Authorization: Bearer $OPENOPS_API_TOKEN\" \"$OPENOPS_API_BASE/api/deployment-events\""
            .to_string(),
    );
    lines.push("# Filter by cluster: ?cluster_id=UUID  or by rollout: &namespace=X&rollout_name=Y".to_string());
    lines.push("```".to_string());
    lines.push(String::new());
    lines.push("### Issues & Observability".to_string());
    lines.push("```bash".to_string());
    lines.push("# Active issues / alerts".to_string());
    lines.push("curl -s -H \"Authorization: Bearer $OPENOPS_API_TOKEN\" \"$OPENOPS_API_BASE/api/issues\"".to_string());
    lines.push("# Issue detail".to_string());
    lines.push(
        "curl -s -H \"Authorization: Bearer $OPENOPS_API_TOKEN\" \"$OPENOPS_API_BASE/api/issues/{id}\"".to_string(),
    );
    lines.push("# Start root cause analysis on an issue".to_string());
    lines.push(
        "curl -s -X POST -H \"Authorization: Bearer $OPENOPS_API_TOKEN\" \"$OPENOPS_API_BASE/api/issues/{id}/rca\""
            .to_string(),
    );
    lines.push("# Telemetry config (Grafana/Mimir/Loki/Tempo endpoints)".to_string());
    lines.push(
        "curl -s -H \"Authorization: Bearer $OPENOPS_API_TOKEN\" \"$OPENOPS_API_BASE/api/telemetry\"".to_string(),
    );
    lines.push("# Dashboard stats".to_string());
    lines.push(
        "curl -s -H \"Authorization: Bearer $OPENOPS_API_TOKEN\" \"$OPENOPS_API_BASE/api/dashboard/stats\"".to_string(),
    );
    lines.push("```".to_string());
    lines.push(String::new());
    lines.push("### Knowledge & Glossary".to_string());
    lines.push("```bash".to_string());
    lines.push("# Glossary (internal terminology)".to_string());
    lines
        .push("curl -s -H \"Authorization: Bearer $OPENOPS_API_TOKEN\" \"$OPENOPS_API_BASE/api/glossary\"".to_string());
    lines.push("# Knowledge base (runbooks, docs)".to_string());
    lines.push(
        "curl -s -H \"Authorization: Bearer $OPENOPS_API_TOKEN\" \"$OPENOPS_API_BASE/api/knowledge\"".to_string(),
    );
    lines.push("```".to_string());
    lines.push(String::new());
    lines.push("### Integrations".to_string());
    lines.push("```bash".to_string());
    lines.push("# Notification channels (Slack, webhook, etc.)".to_string());
    lines
        .push("curl -s -H \"Authorization: Bearer $OPENOPS_API_TOKEN\" \"$OPENOPS_API_BASE/api/channels\"".to_string());
    lines.push("# LLM providers".to_string());
    lines.push(
        "curl -s -H \"Authorization: Bearer $OPENOPS_API_TOKEN\" \"$OPENOPS_API_BASE/api/providers\"".to_string(),
    );
    lines.push("# MCP servers".to_string());
    lines.push("curl -s -H \"Authorization: Bearer $OPENOPS_API_TOKEN\" \"$OPENOPS_API_BASE/api/mcp\"".to_string());
    lines.push("# Scheduled jobs".to_string());
    lines.push(
        "curl -s -H \"Authorization: Bearer $OPENOPS_API_TOKEN\" \"$OPENOPS_API_BASE/api/scheduled-jobs\"".to_string(),
    );
    lines.push("# Pipeline repos (Git)".to_string());
    lines.push(
        "curl -s -H \"Authorization: Bearer $OPENOPS_API_TOKEN\" \"$OPENOPS_API_BASE/api/pipeline/repos\"".to_string(),
    );
    lines.push("```".to_string());
    lines.push(String::new());
    lines.push("## API Usage Rules".to_string());
    lines.push(String::new());
    lines.push("- Always call the relevant API FIRST before answering.".to_string());
    lines.push("- **Empty result handling**: If an API returns `[]` or `null`, that is the definitive answer — the data does not exist. Report this to the user immediately. Do NOT retry the same endpoint with different parameters, do NOT try alternative query approaches, do NOT loop. An empty array means zero records, not an error.".to_string());
    lines.push("- **Error handling**: Only retry on HTTP 5xx errors (max 1 retry). For 4xx errors, report the error to the user.".to_string());
    lines.push(String::new());

    // ─── Jira integration instructions (only if tenant has enabled Jira channel) ──
    let has_jira = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM channels WHERE platform = 'jira' AND enabled = true AND tenant_id = $1)",
    )
    .bind(auth_user.tenant_id)
    .fetch_one(&state.pool)
    .await
    .unwrap_or(false);

    if has_jira {
        lines.push("### Jira Integration".to_string());
        lines.push(String::new());
        lines.push("When performing **infrastructure changes** (create/delete/modify resources, security fixes, config changes), you SHOULD:".to_string());
        lines.push("1. Create a Jira issue BEFORE starting the work".to_string());
        lines.push("2. Execute the task".to_string());
        lines.push("3. Update the Jira issue with results when done".to_string());
        lines.push(String::new());
        lines.push("```bash".to_string());
        lines.push("# Create Jira issue (returns key like OPS-123)".to_string());
        lines.push("curl -s -X POST -H \"Authorization: Bearer $OPENOPS_API_TOKEN\" \\".to_string());
        lines.push("  -H \"Content-Type: application/json\" \\".to_string());
        lines.push("  -d '{\"summary\":\"...\",\"description\":\"...\",\"issue_type\":\"Task\"}' \\".to_string());
        lines.push("  \"$OPENOPS_API_BASE/api/jira/create\"".to_string());
        lines.push(String::new());
        lines.push("# Transition to Done with comment".to_string());
        lines.push("curl -s -X POST -H \"Authorization: Bearer $OPENOPS_API_TOKEN\" \\".to_string());
        lines.push("  -H \"Content-Type: application/json\" \\".to_string());
        lines.push("  -d '{\"status\":\"Done\",\"comment\":\"...results...\"}' \\".to_string());
        lines.push("  \"$OPENOPS_API_BASE/api/jira/{key}/transition\"".to_string());
        lines.push(String::new());
        lines.push("# Add comment to existing issue".to_string());
        lines.push("curl -s -X POST -H \"Authorization: Bearer $OPENOPS_API_TOKEN\" \\".to_string());
        lines.push("  -H \"Content-Type: application/json\" \\".to_string());
        lines.push("  -d '{\"comment\":\"...\"}' \\".to_string());
        lines.push("  \"$OPENOPS_API_BASE/api/jira/{key}/comment\"".to_string());
        lines.push("```".to_string());
        lines.push(String::new());
        lines.push(
            "Only create Jira issues for **actual changes** (infra provisioning, security fixes, config changes)."
                .to_string(),
        );
        lines.push("Do NOT create issues for read-only queries, status checks, or information lookups.".to_string());
        lines.push(String::new());
    }

    // Inject glossary inline as quick reference (so agent doesn't need API call for common terms)
    if let Ok(terms) = sqlx::query_as::<_, (String, Option<String>, Option<String>)>(
        "SELECT term, full_name, description FROM glossary WHERE account_id = ANY($1) OR account_id IS NULL LIMIT 50",
    )
    .bind(&account_ids)
    .fetch_all(&state.pool)
    .await
        && !terms.is_empty()
    {
        lines.push("## Quick Glossary Reference".to_string());
        lines.push(String::new());
        for (term, full_name, desc) in terms {
            let full = full_name.unwrap_or_default();
            let d = desc.unwrap_or_default();
            lines.push(format!("- **{}** ({}): {}", term, full, d));
        }
        lines.push(String::new());
    }

    // Inject cloud accounts
    if let Ok(accounts) = sqlx::query_as::<_, (String, String, Option<String>, Option<String>, Vec<String>)>(
        "SELECT provider, name, account_id, role_arn, regions FROM cloud_accounts WHERE id = ANY($1) AND is_mock = false LIMIT 20",
    )
    .bind(&account_ids)
    .fetch_all(&state.pool)
    .await
        && !accounts.is_empty()
    {
        lines.push("## Available Cloud Accounts".to_string());
        lines.push(String::new());
        for (provider, name, account_id, role_arn, regions) in &accounts {
            let aid = account_id.as_deref().unwrap_or("-");
            let regions_str = if regions.is_empty() { "ALL".to_string() } else { regions.join(", ") };
            let role_info = role_arn.as_deref().map(|r| format!(", Role: {}", r)).unwrap_or_default();
            lines.push(format!("- {} ({}) — Account: {}, Regions: [{}]{}", name, provider, aid, regions_str, role_info));
        }
        lines.push(String::new());
    }

    // Inject clusters
    if let Ok(clusters) = sqlx::query_as::<_, (String, String, String, Option<String>, Option<String>, String)>(
        "SELECT name, cloud, cluster_type, region, account_id, status FROM clusters WHERE tenant_id IS NOT DISTINCT FROM $1 LIMIT 20",
    )
    .bind(auth_user.tenant_id)
    .fetch_all(&state.pool)
    .await
        && !clusters.is_empty()
    {
        lines.push("## Kubernetes Clusters".to_string());
        lines.push(String::new());
        for (name, cloud, ctype, region, account_id, status) in &clusters {
            let r = region.as_deref().unwrap_or("?");
            let aid = account_id.as_deref().unwrap_or("-");
            lines.push(format!("- {} ({}/{}) — Region: {}, Account: {}, Status: {}", name, cloud, ctype, r, aid, status));
        }
        lines.push(String::new());
    }

    // ─── Observability note ──────────────────────────────────────
    // Full prediction + RCA instructions are in project CLAUDE.md.
    // Here we just remind the agent how to discover telemetry endpoints at runtime.
    lines.push("## Observability".to_string());
    lines.push(String::new());
    lines.push(
        "Prediction and RCA instructions are defined in the project CLAUDE.md. To get live telemetry endpoints:"
            .to_string(),
    );
    lines.push(String::new());
    lines.push("```bash".to_string());
    lines.push("# Fetch configured telemetry provider + endpoints".to_string());
    lines.push(
        "curl -s -H \"Authorization: Bearer $OPENOPS_API_TOKEN\" \"$OPENOPS_API_BASE/api/telemetry\"".to_string(),
    );
    lines.push("```".to_string());
    lines.push(String::new());

    let content = lines.join("\n");
    let claude_md_path = user_work_dir.join("CLAUDE.md");

    // Only write if content changed (avoid unnecessary disk writes)
    let should_write = match tokio::fs::read_to_string(&claude_md_path).await {
        Ok(existing) => existing != content,
        Err(_) => true,
    };

    if should_write {
        // Ensure directory exists
        if let Err(e) = tokio::fs::create_dir_all(user_work_dir).await {
            tracing::warn!("Failed to create user work dir {:?}: {}", user_work_dir, e);
            return;
        }
        if let Err(e) = tokio::fs::write(&claude_md_path, &content).await {
            tracing::warn!("Failed to write CLAUDE.md to {:?}: {}", claude_md_path, e);
        } else {
            tracing::info!("Wrote CLAUDE.md ({} bytes) to {:?}", content.len(), claude_md_path);
        }
    }
}

/// Build per-user `.claude/skills/` directory with symlinks to authorized skills only.
/// This ensures Claude CLI's native skill discovery (`/skill-name`) only sees
/// skills the user has permission to access (private + tenant-public).
async fn setup_user_skill_symlinks(
    state: &AppState,
    user_work_dir: &std::path::Path,
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
                if src.exists()
                    && let Err(e) = tokio::fs::symlink(src, &link).await
                {
                    tracing::warn!("Failed to symlink skill {:?} -> {:?}: {}", link, src, e);
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
           AND last_active_at > NOW() - INTERVAL '24 hours'
           ORDER BY last_active_at DESC
           LIMIT 20"#,
    )
    .bind(auth_user.user_id)
    .bind(auth_user.tenant_id)
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(sessions))
}

/// Build provider-specific environment variables for Claude CLI
fn build_provider_env_vars(provider_type: &str, config: &serde_json::Value) -> Vec<(String, String)> {
    let mut env = Vec::new();
    match provider_type {
        "bedrock" => {
            env.push(("CLAUDE_CODE_USE_BEDROCK".to_string(), "1".to_string()));
            if let Some(r) = config.get("region").and_then(|v| v.as_str()) {
                env.push(("AWS_REGION".to_string(), r.to_string()));
            }
        }
        "gateway" => {
            if let Some(u) = config.get("base_url").and_then(|v| v.as_str())
                && !u.is_empty()
            {
                // Claude CLI auto-appends /v1/messages, so strip trailing /v1 if present
                let base = u.trim_end_matches('/');
                let base = base.strip_suffix("/v1").unwrap_or(base);
                env.push(("ANTHROPIC_BASE_URL".to_string(), base.to_string()));
            }
            if let Some(k) = config.get("api_key").and_then(|v| v.as_str())
                && !k.is_empty()
            {
                env.push(("ANTHROPIC_API_KEY".to_string(), k.to_string()));
            }
        }
        _ => {}
    }
    env
}

/// Tools to block in readonly mode: all file-write tools
const DEFAULT_DISALLOWED_TOOLS: &[&str] = &["Write", "Edit", "NotebookEdit"];

/// Allowed tool patterns for readonly mode: restrict Bash to read-only
const DEFAULT_ALLOWED_TOOLS: &[&str] = &["Bash(read-only:*)"];

/// Provider configuration extracted from DB
struct ProviderSettings {
    model: String,
    timeout: Duration,
    max_turns: u32,
    env_vars: Vec<(String, String)>,
    permission_mode: String,
    disallowed_tools: Vec<String>,
    allowed_tools: Vec<String>,
}

/// Load model + timeout + max_turns + provider env vars + permission settings from providers table.
/// If provider_id is given, use that specific provider; otherwise use the tenant's default.
async fn load_provider_config(
    state: &AppState,
    tenant_id: Option<uuid::Uuid>,
    provider_id: Option<uuid::Uuid>,
) -> ProviderSettings {
    let row = if let Some(pid) = provider_id {
        // Use specific provider
        sqlx::query_as::<_, (String, serde_json::Value)>("SELECT provider_type, config FROM providers WHERE id = $1")
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
        let max_turns = config.get("max_turns").and_then(|v| v.as_u64()).unwrap_or(25) as u32;
        let provider_envs = build_provider_env_vars(&provider_type, &config);

        // Permission settings: config stores "readonly" or "bypassPermissions"
        // Maps to CLI --permission-mode: readonly→"default", bypassPermissions→"bypassPermissions"
        let permission_mode = config
            .get("permission_mode")
            .and_then(|v| v.as_str())
            .unwrap_or("readonly")
            .to_string();

        // Map config value to CLI --permission-mode flag
        // readonly → "default": blocks reads outside workspace (CWD), no interactive terminal
        // bypassPermissions → "bypassPermissions": full access
        let cli_permission_mode = if permission_mode == "bypassPermissions" {
            "bypassPermissions".to_string()
        } else {
            "default".to_string()
        };

        let (disallowed_tools, allowed_tools) = if permission_mode == "bypassPermissions" {
            // Full access — no restrictions
            (Vec::new(), Vec::new())
        } else {
            // readonly: block write tools, restrict Bash to read-only
            let disallowed = config
                .get("disallowed_tools")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_else(|| DEFAULT_DISALLOWED_TOOLS.iter().map(|s| s.to_string()).collect());
            let allowed = DEFAULT_ALLOWED_TOOLS.iter().map(|s| s.to_string()).collect();
            (disallowed, allowed)
        };

        ProviderSettings {
            model,
            timeout: Duration::from_millis(timeout_ms),
            max_turns,
            env_vars: provider_envs,
            permission_mode: cli_permission_mode,
            disallowed_tools,
            allowed_tools,
        }
    } else {
        ProviderSettings {
            model: state.config.claude_model.clone(),
            timeout: Duration::from_millis(state.config.claude_timeout_ms),
            max_turns: 25,
            env_vars: Vec::new(),
            permission_mode: "default".to_string(),
            disallowed_tools: DEFAULT_DISALLOWED_TOOLS.iter().map(|s| s.to_string()).collect(),
            allowed_tools: DEFAULT_ALLOWED_TOOLS.iter().map(|s| s.to_string()).collect(),
        }
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
    let scans_dir = base.join("scans");

    let mut files = Vec::new();
    // User-specific workspace files
    collect_workspace_files(&user_dir, &user_dir, &mut files).await;
    // Shared scan reports (prefix paths with "scans/")
    let mut scan_files = Vec::new();
    collect_workspace_files(&scans_dir, &scans_dir, &mut scan_files).await;
    for mut f in scan_files {
        f.name = format!("scans/{}", f.name);
        files.push(f);
    }
    files.sort_by(|a, b| b.modified.cmp(&a.modified));
    Ok(Json(files))
}

/// Recursively collect files from workspace, using relative paths from root
async fn collect_workspace_files(root: &std::path::Path, dir: &std::path::Path, files: &mut Vec<WorkspaceFile>) {
    let Ok(mut entries) = tokio::fs::read_dir(dir).await else {
        return;
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name().to_string_lossy().to_string();
        // Skip hidden dirs (.claude, .git, etc.) and internal files
        if name.starts_with('.') || name == "CLAUDE.md" {
            continue;
        }
        let path = entry.path();
        if let Ok(meta) = entry.metadata().await {
            let rel_path = path
                .strip_prefix(root)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or(name.clone());
            if meta.is_dir() {
                Box::pin(collect_workspace_files(root, &path, files)).await;
            } else {
                files.push(WorkspaceFile {
                    name: rel_path,
                    size: meta.len(),
                    modified: chrono::DateTime::<chrono::Utc>::from(
                        meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH),
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

    // Support both user files and shared scan reports
    let (file_path, allowed_root) = if filename.starts_with("scans/") {
        (base.join(&filename), base.join("scans"))
    } else {
        let user_dir = base.join("users").join(auth_user.user_id.to_string());
        (user_dir.join(&filename), user_dir)
    };

    // Ensure resolved path is still under allowed root (prevent symlink escape)
    if let (Ok(resolved), Ok(root_resolved)) = (file_path.canonicalize(), allowed_root.canonicalize())
        && !resolved.starts_with(&root_resolved)
    {
        return Err(AppError::BadRequest("Invalid path".to_string()));
    }

    if !file_path.exists() || file_path.is_dir() {
        return Err(AppError::NotFound("File not found".to_string()));
    }

    let bytes = tokio::fs::read(&file_path)
        .await
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
            (
                http::header::CONTENT_DISPOSITION,
                &format!(
                    "attachment; filename=\"{}\"",
                    filename.split('/').next_back().unwrap_or(&filename)
                ),
            ),
        ],
        bytes,
    )
        .into_response())
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

    let (file_path, allowed_root) = if filename.starts_with("scans/") {
        (base.join(&filename), base.join("scans"))
    } else {
        let user_dir = base.join("users").join(auth_user.user_id.to_string());
        (user_dir.join(&filename), user_dir)
    };

    // Ensure resolved path is still under allowed root
    if let (Ok(resolved), Ok(root_resolved)) = (file_path.canonicalize(), allowed_root.canonicalize())
        && !resolved.starts_with(&root_resolved)
    {
        return Err(AppError::BadRequest("Invalid path".to_string()));
    }

    if !file_path.exists() {
        return Err(AppError::NotFound("File not found".to_string()));
    }

    if file_path.is_dir() {
        tokio::fs::remove_dir_all(&file_path)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to delete directory: {}", e)))?;
    } else {
        tokio::fs::remove_file(&file_path)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to delete file: {}", e)))?;
    }

    // Clean up empty parent dirs (up to user_dir)
    let mut current = file_path.parent().map(|p| p.to_path_buf());
    let user_dir_resolved = allowed_root.canonicalize().unwrap_or(allowed_root.clone());
    while let Some(p) = current {
        let p_resolved = p.canonicalize().unwrap_or(p.clone());
        if p_resolved == user_dir_resolved {
            break;
        }
        // Try to remove — only succeeds if empty
        if tokio::fs::remove_dir(&p).await.is_err() {
            break;
        }
        current = p.parent().map(|pp| pp.to_path_buf());
    }

    Ok(Json(serde_json::json!({"message": "Deleted"})))
}
