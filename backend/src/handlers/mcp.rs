use axum::{
    Json,
    extract::{Path, State},
};
use uuid::Uuid;

use crate::AppState;
use crate::error::{AppError, AppResult};
use crate::middleware::auth::AuthUser;
use crate::models::mcp::{CreateMcpServerRequest, McpServer, TestMcpServerRequest, UpdateMcpServerRequest};

/// GET /api/mcp
/// Super admin: all. Normal user: own private + tenant public
pub async fn list(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
) -> AppResult<Json<Vec<McpServer>>> {
    let rows = if auth_user.is_super_admin() {
        sqlx::query_as::<_, McpServer>("SELECT * FROM mcp_servers ORDER BY name")
            .fetch_all(&state.pool)
            .await?
    } else {
        sqlx::query_as::<_, McpServer>(
            r#"SELECT * FROM mcp_servers
               WHERE (user_id = $1) OR (user_id IS NULL AND tenant_id IS NOT DISTINCT FROM $2)
               ORDER BY name"#,
        )
        .bind(auth_user.user_id)
        .bind(auth_user.tenant_id)
        .fetch_all(&state.pool)
        .await?
    };
    Ok(Json(rows))
}

/// POST /api/mcp
pub async fn create(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Json(req): Json<CreateMcpServerRequest>,
) -> AppResult<Json<McpServer>> {
    if req.name.trim().is_empty() {
        return Err(AppError::BadRequest("Name is required".to_string()));
    }

    // Validate transport type
    let transport = match req.transport_type.as_str() {
        "stdio" | "sse" | "http" => req.transport_type.clone(),
        _ => "stdio".to_string(),
    };

    // For stdio, command is required; for sse/http, url is required
    if transport == "stdio" && req.command.trim().is_empty() {
        return Err(AppError::BadRequest(
            "Command is required for STDIO transport".to_string(),
        ));
    }
    if (transport == "sse" || transport == "http") && req.url.as_deref().unwrap_or("").is_empty() {
        return Err(AppError::BadRequest(
            "URL is required for SSE/HTTP transport".to_string(),
        ));
    }

    let visibility = match req.visibility.as_str() {
        "public" | "private" => req.visibility.clone(),
        _ => "public".to_string(),
    };

    let tenant_id = auth_user.tenant_id;
    let user_id = if visibility == "private" {
        Some(auth_user.user_id)
    } else {
        None
    };

    let row = sqlx::query_as::<_, McpServer>(
        r#"INSERT INTO mcp_servers (name, command, args, env, enabled, tenant_id, user_id, created_by, visibility, transport_type, url, headers, description)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
           RETURNING *"#,
    )
    .bind(&req.name)
    .bind(&req.command)
    .bind(&req.args)
    .bind(&req.env)
    .bind(req.enabled)
    .bind(tenant_id)
    .bind(user_id)
    .bind(auth_user.user_id)
    .bind(&visibility)
    .bind(&transport)
    .bind(&req.url)
    .bind(&req.headers)
    .bind(&req.description)
    .fetch_one(&state.pool)
    .await?;

    Ok(Json(row))
}

/// PUT /api/mcp/:id
pub async fn update(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateMcpServerRequest>,
) -> AppResult<Json<McpServer>> {
    let existing = sqlx::query_as::<_, McpServer>("SELECT * FROM mcp_servers WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("MCP server not found".to_string()))?;

    if !auth_user.is_super_admin() {
        let has_access = existing.user_id == Some(auth_user.user_id)
            || (existing.visibility == "public" && existing.tenant_id == auth_user.tenant_id);
        if !has_access {
            return Err(AppError::Forbidden("Access denied".to_string()));
        }
    }

    let row = sqlx::query_as::<_, McpServer>(
        r#"UPDATE mcp_servers SET
           name = COALESCE($2, name),
           command = COALESCE($3, command),
           args = COALESCE($4, args),
           env = COALESCE($5, env),
           enabled = COALESCE($6, enabled),
           transport_type = COALESCE($7, transport_type),
           url = COALESCE($8, url),
           headers = COALESCE($9, headers),
           description = COALESCE($10, description),
           updated_at = NOW()
           WHERE id = $1
           RETURNING *"#,
    )
    .bind(id)
    .bind(&req.name)
    .bind(&req.command)
    .bind(&req.args)
    .bind(&req.env)
    .bind(req.enabled)
    .bind(&req.transport_type)
    .bind(&req.url)
    .bind(&req.headers)
    .bind(&req.description)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("MCP server not found".to_string()))?;

    Ok(Json(row))
}

/// DELETE /api/mcp/:id
pub async fn delete(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let existing = sqlx::query_as::<_, McpServer>("SELECT * FROM mcp_servers WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("MCP server not found".to_string()))?;

    if !auth_user.is_super_admin() {
        let has_access = existing.user_id == Some(auth_user.user_id)
            || (existing.visibility == "public" && existing.tenant_id == auth_user.tenant_id);
        if !has_access {
            return Err(AppError::Forbidden("Access denied".to_string()));
        }
    }

    sqlx::query("DELETE FROM mcp_servers WHERE id = $1")
        .bind(id)
        .execute(&state.pool)
        .await?;

    Ok(Json(serde_json::json!({"message": "MCP server deleted"})))
}

/// POST /api/mcp/test — test connectivity to an MCP server.
/// Discovers tools on success and saves them to DB if server_id is provided.
/// For SSE/HTTP: JSON-RPC initialize + tools/list.
/// For stdio: spawns `claude mcp list` with a temp .mcp.json.
pub async fn test(
    _auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Json(req): Json<TestMcpServerRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let result = match req.transport_type.as_str() {
        "sse" | "http" => test_http_server(&req).await,
        _ => test_stdio_server(&state, &req).await,
    }?;

    // If test succeeded and server_id provided, save discovered tools to DB
    if let (Some(server_id), Some(true)) = (&req.server_id, result.get("success").and_then(|v| v.as_bool()))
        && let Ok(id) = uuid::Uuid::parse_str(server_id)
    {
        let tools = result.get("tools").cloned().unwrap_or(serde_json::json!([]));
        let _ = sqlx::query("UPDATE mcp_servers SET tools = $1, updated_at = NOW() WHERE id = $2")
            .bind(&tools)
            .bind(id)
            .execute(&state.pool)
            .await;
        tracing::info!(
            "Saved {} tools for MCP server {}",
            tools.as_array().map(|a| a.len()).unwrap_or(0),
            id
        );
    }

    Ok(result)
}

/// Extract JSON from a response that may be SSE-formatted.
/// SSE responses look like: "event: message\ndata: {json}\n\n"
/// Plain JSON is returned as-is.
fn extract_sse_json(text: &str) -> String {
    // Try to find "data: " lines and concatenate their JSON payloads
    let data_lines: Vec<&str> = text.lines().filter_map(|line| line.strip_prefix("data: ")).collect();

    if !data_lines.is_empty() {
        // SSE may split JSON across multiple data lines (rare), join them
        data_lines.join("")
    } else {
        // Not SSE — return as-is (plain JSON)
        text.to_string()
    }
}

/// Test HTTP/SSE MCP server: JSON-RPC initialize → tools/list.
/// Returns success + discovered tools list.
async fn test_http_server(req: &TestMcpServerRequest) -> AppResult<Json<serde_json::Value>> {
    let url = match &req.url {
        Some(u) if !u.is_empty() => u.clone(),
        _ => {
            return Ok(Json(
                serde_json::json!({"success": false, "error": "URL is required", "tools": []}),
            ));
        }
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| AppError::Internal(format!("HTTP client error: {e}")))?;

    // Step 1: initialize
    let init_body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-03-26",
            "capabilities": {},
            "clientInfo": { "name": "ops-test", "version": "0.1.0" }
        }
    });

    let mut init_req = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .json(&init_body);

    if let Some(obj) = req.headers.as_object() {
        for (key, val) in obj {
            if let Some(v) = val.as_str() {
                init_req = init_req.header(key, v);
            }
        }
    }

    let init_resp = match init_req.send().await {
        Ok(r) => r,
        Err(e) => {
            return Ok(Json(
                serde_json::json!({"success": false, "error": format!("Connection failed: {e}"), "tools": []}),
            ));
        }
    };

    let status = init_resp.status().as_u16();
    let session_id = init_resp
        .headers()
        .get("mcp-session-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    if status != 200 {
        let body = init_resp.text().await.unwrap_or_default();
        if status == 401 || status == 403 {
            return Ok(Json(
                serde_json::json!({"success": false, "error": format!("HTTP {status}: check auth token"), "tools": []}),
            ));
        }
        return Ok(Json(
            serde_json::json!({"success": false, "error": format!("HTTP {status}: {body}"), "tools": []}),
        ));
    }

    // Consume init body
    let _ = init_resp.text().await;

    // Step 2: tools/list
    let tools_body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list",
        "params": {}
    });

    let mut tools_req = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .json(&tools_body);

    if let Some(sid) = &session_id {
        tools_req = tools_req.header("Mcp-Session-Id", sid);
    }
    if let Some(obj) = req.headers.as_object() {
        for (key, val) in obj {
            if let Some(v) = val.as_str() {
                tools_req = tools_req.header(key, v);
            }
        }
    }

    let tools_resp = match tools_req.send().await {
        Ok(r) => r,
        Err(_) => {
            // initialize succeeded but tools/list failed — still success
            return Ok(Json(
                serde_json::json!({"success": true, "message": "Connected but tools/list failed", "tools": []}),
            ));
        }
    };

    let tools_status = tools_resp.status().as_u16();
    let tools_text = tools_resp.text().await.unwrap_or_default();

    if tools_status != 200 {
        return Ok(Json(
            serde_json::json!({"success": true, "message": "Connected (tools/list unavailable)", "tools": []}),
        ));
    }

    // Parse tools — response may be plain JSON or SSE (event: message\ndata: {...})
    let json_text = extract_sse_json(&tools_text);
    let parsed: serde_json::Value = serde_json::from_str(&json_text).unwrap_or_default();
    let tools_arr = parsed
        .get("result")
        .and_then(|r| r.get("tools"))
        .and_then(|t| t.as_array())
        .cloned()
        .unwrap_or_default();

    let tools: Vec<serde_json::Value> = tools_arr
        .iter()
        .filter_map(|t| {
            let name = t.get("name")?.as_str()?;
            let desc = t.get("description").and_then(|d| d.as_str()).unwrap_or("");
            Some(serde_json::json!({"name": name, "description": desc}))
        })
        .collect();

    let count = tools.len();
    Ok(Json(serde_json::json!({
        "success": true,
        "message": format!("Connected — {count} tools discovered"),
        "tools": tools
    })))
}

/// GET /api/mcp/:id/tools — discover tools exposed by an MCP server.
/// For HTTP: JSON-RPC initialize → tools/list.
/// For stdio: spawn `claude mcp list` and parse output.
pub async fn list_tools(
    _auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Vec<McpToolInfo>>> {
    let server = sqlx::query_as::<_, crate::models::mcp::McpServer>("SELECT * FROM mcp_servers WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("MCP server not found".to_string()))?;

    let tools = match server.transport_type.as_str() {
        "sse" | "http" => discover_http_tools(&server).await?,
        _ => discover_stdio_tools(&state, &server).await?,
    };

    Ok(Json(tools))
}

#[derive(Debug, serde::Serialize)]
pub struct McpToolInfo {
    pub name: String,
    pub description: String,
    pub server_id: Uuid,
    pub server_name: String,
}

/// Discover tools from HTTP/SSE MCP server via JSON-RPC: initialize → tools/list.
async fn discover_http_tools(server: &crate::models::mcp::McpServer) -> Result<Vec<McpToolInfo>, AppError> {
    let url = server.url.as_deref().unwrap_or("");
    if url.is_empty() {
        return Err(AppError::BadRequest("Server has no URL".to_string()));
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| AppError::Internal(format!("HTTP client error: {e}")))?;

    // Step 1: initialize — get session ID
    let init_body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-03-26",
            "capabilities": {},
            "clientInfo": { "name": "ops", "version": "0.1.0" }
        }
    });

    let mut init_req = client
        .post(url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .json(&init_body);

    if let Some(obj) = server.headers.as_object() {
        for (key, val) in obj {
            if let Some(v) = val.as_str() {
                init_req = init_req.header(key, v);
            }
        }
    }

    let init_resp = init_req
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("Failed to connect to MCP server: {e}")))?;

    let session_id = init_resp
        .headers()
        .get("mcp-session-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let init_status = init_resp.status().as_u16();
    if init_status != 200 {
        let body = init_resp.text().await.unwrap_or_default();
        return Err(AppError::Internal(format!(
            "MCP initialize failed: HTTP {init_status}: {body}"
        )));
    }

    // Consume init response body (discard)
    let _ = init_resp.text().await;

    // Step 2: tools/list
    let tools_body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list",
        "params": {}
    });

    let mut tools_req = client
        .post(url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .json(&tools_body);

    if let Some(sid) = &session_id {
        tools_req = tools_req.header("Mcp-Session-Id", sid);
    }
    if let Some(obj) = server.headers.as_object() {
        for (key, val) in obj {
            if let Some(v) = val.as_str() {
                tools_req = tools_req.header(key, v);
            }
        }
    }

    let tools_resp = tools_req
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("Failed to list tools: {e}")))?;

    let tools_status = tools_resp.status().as_u16();
    let tools_text = tools_resp.text().await.unwrap_or_default();

    if tools_status != 200 {
        return Err(AppError::Internal(format!(
            "tools/list failed: HTTP {tools_status}: {tools_text}"
        )));
    }

    // Parse JSON-RPC response — may be plain JSON or SSE (event: message\ndata: {...})
    let json_text = extract_sse_json(&tools_text);
    let parsed: serde_json::Value = serde_json::from_str(&json_text)
        .map_err(|e| AppError::Internal(format!("Invalid JSON from tools/list: {e}")))?;

    let tools_arr = parsed
        .get("result")
        .and_then(|r| r.get("tools"))
        .and_then(|t| t.as_array())
        .cloned()
        .unwrap_or_default();

    let tools: Vec<McpToolInfo> = tools_arr
        .iter()
        .filter_map(|t| {
            let name = t.get("name")?.as_str()?.to_string();
            let description = t.get("description").and_then(|d| d.as_str()).unwrap_or("").to_string();
            Some(McpToolInfo {
                name,
                description,
                server_id: server.id,
                server_name: server.name.clone(),
            })
        })
        .collect();

    tracing::info!("Discovered {} tools from HTTP server '{}'", tools.len(), server.name);
    Ok(tools)
}

/// Discover tools from stdio MCP server by spawning `claude mcp list`.
async fn discover_stdio_tools(
    state: &AppState,
    server: &crate::models::mcp::McpServer,
) -> Result<Vec<McpToolInfo>, AppError> {
    // Build .mcp.json config
    let mut entry = serde_json::Map::new();
    entry.insert("command".to_string(), serde_json::json!(server.command));
    if server.args != serde_json::json!([]) {
        entry.insert("args".to_string(), server.args.clone());
    }
    if server.env != serde_json::json!({}) {
        entry.insert("env".to_string(), server.env.clone());
    }

    let config = serde_json::json!({ "mcpServers": { &server.name: serde_json::Value::Object(entry) } });

    let tmp_dir = std::env::temp_dir().join(format!("ops-mcp-tools-{}", uuid::Uuid::new_v4()));
    tokio::fs::create_dir_all(&tmp_dir)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to create temp dir: {e}")))?;
    let config_file = tmp_dir.join(".mcp.json");
    tokio::fs::write(&config_file, serde_json::to_string_pretty(&config).unwrap_or_default())
        .await
        .map_err(|e| AppError::Internal(format!("Failed to write .mcp.json: {e}")))?;

    let output = tokio::process::Command::new(&state.config.claude_bin)
        .args(["mcp", "list"])
        .current_dir(&tmp_dir)
        .output()
        .await
        .map_err(|e| AppError::Internal(format!("Failed to spawn claude CLI: {e}")))?;

    let _ = tokio::fs::remove_dir_all(&tmp_dir).await;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}{}", stdout, stderr);

    // Parse tool names from `claude mcp list` output
    // Format: "  - tool_name: description" under each server
    let mut tools = Vec::new();
    let mut in_tools_section = false;
    for line in combined.lines() {
        let trimmed = line.trim();
        // Server line: "server_name: ✓ Connected (N tools)"
        if trimmed.contains("Connected") && trimmed.contains("tools") {
            in_tools_section = true;
            continue;
        }
        // Tool line: "  - tool_name: description"
        if in_tools_section && trimmed.starts_with("- ") {
            let content = &trimmed[2..];
            let (name, desc) = if let Some(colon) = content.find(':') {
                (
                    content[..colon].trim().to_string(),
                    content[colon + 1..].trim().to_string(),
                )
            } else {
                (content.trim().to_string(), String::new())
            };
            if !name.is_empty() {
                tools.push(McpToolInfo {
                    name,
                    description: desc,
                    server_id: server.id,
                    server_name: server.name.clone(),
                });
            }
        }
        // Empty line or new server = end of tools section
        if in_tools_section && trimmed.is_empty() {
            in_tools_section = false;
        }
    }

    tracing::info!(
        "Discovered {} tools from stdio server '{}': {:?}",
        tools.len(),
        server.name,
        combined.trim()
    );
    Ok(tools)
}

/// Test stdio MCP server by spawning `claude mcp list` with a temp .mcp.json.
/// Parses output to extract tool names.
async fn test_stdio_server(state: &AppState, req: &TestMcpServerRequest) -> AppResult<Json<serde_json::Value>> {
    let mut entry = serde_json::Map::new();
    entry.insert("command".to_string(), serde_json::json!(req.command));
    if req.args != serde_json::json!([]) {
        entry.insert("args".to_string(), req.args.clone());
    }
    if req.env != serde_json::json!({}) {
        entry.insert("env".to_string(), req.env.clone());
    }

    let config = serde_json::json!({ "mcpServers": { &req.name: serde_json::Value::Object(entry) } });

    // Write .mcp.json in a temp directory
    let tmp_dir = std::env::temp_dir().join(format!("ops-mcp-test-{}", uuid::Uuid::new_v4()));
    tokio::fs::create_dir_all(&tmp_dir)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to create temp dir: {e}")))?;
    let config_file = tmp_dir.join(".mcp.json");
    tokio::fs::write(&config_file, serde_json::to_string_pretty(&config).unwrap_or_default())
        .await
        .map_err(|e| AppError::Internal(format!("Failed to write .mcp.json: {e}")))?;

    let output = tokio::process::Command::new(&state.config.claude_bin)
        .args(["mcp", "list"])
        .current_dir(&tmp_dir)
        .output()
        .await
        .map_err(|e| AppError::Internal(format!("Failed to spawn claude CLI: {e}")))?;

    let _ = tokio::fs::remove_dir_all(&tmp_dir).await;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}{}", stdout, stderr);

    tracing::info!("MCP test output: {}", combined.trim());

    let connected = combined.contains("\u{2713}") || combined.contains("Connected");

    // Parse tool names from output: "  - tool_name: description"
    let mut tools = Vec::new();
    let mut in_tools = false;
    for line in combined.lines() {
        let trimmed = line.trim();
        if trimmed.contains("Connected") && trimmed.contains("tools") {
            in_tools = true;
            continue;
        }
        if in_tools && trimmed.starts_with("- ") {
            let content = &trimmed[2..];
            let (name, desc) = if let Some(colon) = content.find(':') {
                (content[..colon].trim(), content[colon + 1..].trim())
            } else {
                (content.trim(), "")
            };
            if !name.is_empty() {
                tools.push(serde_json::json!({"name": name, "description": desc}));
            }
        }
        if in_tools && trimmed.is_empty() {
            in_tools = false;
        }
    }

    if connected {
        let count = tools.len();
        Ok(Json(
            serde_json::json!({"success": true, "message": format!("Connected — {count} tools discovered"), "tools": tools}),
        ))
    } else {
        let err_line = combined
            .lines()
            .find(|l| l.contains("\u{2717}") || l.contains("Failed") || l.contains("Error"))
            .unwrap_or("Connection failed")
            .trim();
        Ok(Json(
            serde_json::json!({"success": false, "error": err_line, "tools": []}),
        ))
    }
}

// ─── GraphRAG proxy ────────────────────────────────────────────────────────────
// Proxies requests to the GraphRAG REST API using credentials from mcp_servers table.
// This avoids exposing the auth token to the frontend.

/// Resolve GraphRAG base URL and auth headers from the mcp_servers table.
/// Looks for an HTTP/SSE server whose URL contains "graphrag".
async fn resolve_graphrag(pool: &sqlx::PgPool) -> Option<(String, String)> {
    let srv = sqlx::query_as::<_, (Option<String>, serde_json::Value)>(
        "SELECT url, headers FROM mcp_servers WHERE enabled = true AND transport_type IN ('http','sse') AND url ILIKE '%graphrag%' LIMIT 1"
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()?;

    let url = srv.0?;
    // Base URL = strip /mcp suffix
    let base = url.trim_end_matches('/').trim_end_matches("/mcp").to_string();
    let auth = srv
        .1
        .get("Authorization")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    Some((base, auth))
}

/// POST /api/graphrag/bbox — proxy to GraphRAG /bbox/batch
/// Body: { context_id, requests: [{chunk_id, bbox_id}] }
pub async fn graphrag_bbox(
    _auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> AppResult<Json<serde_json::Value>> {
    let (base, auth) = resolve_graphrag(&state.pool)
        .await
        .ok_or_else(|| AppError::NotFound("GraphRAG server not configured".to_string()))?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| AppError::Internal(format!("HTTP client error: {e}")))?;

    let resp = client
        .post(format!("{}/bbox/batch", base))
        .header("Authorization", &auth)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("GraphRAG request failed: {e}")))?;

    let status = resp.status().as_u16();
    let data: serde_json::Value = resp.json().await.unwrap_or_default();

    if status != 200 {
        return Err(AppError::Internal(format!("GraphRAG bbox error: HTTP {status}")));
    }

    Ok(Json(data))
}

/// POST /api/graphrag/pdf-url — proxy to GraphRAG /api/presigned-url
/// Body: { context_id, file_path }
pub async fn graphrag_pdf_url(
    _auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> AppResult<Json<serde_json::Value>> {
    let (base, auth) = resolve_graphrag(&state.pool)
        .await
        .ok_or_else(|| AppError::NotFound("GraphRAG server not configured".to_string()))?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| AppError::Internal(format!("HTTP client error: {e}")))?;

    let resp = client
        .post(format!("{}/api/presigned-url", base))
        .header("Authorization", &auth)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("GraphRAG request failed: {e}")))?;

    let status = resp.status().as_u16();
    let data: serde_json::Value = resp.json().await.unwrap_or_default();

    if status != 200 {
        return Err(AppError::Internal(format!("GraphRAG pdf-url error: HTTP {status}")));
    }

    Ok(Json(data))
}

/// GET /api/graphrag/documents/:context_id — proxy to GraphRAG /api/contexts/:context_id/documents
/// Returns list of documents with file_name, s3_key, status, etc.
pub async fn graphrag_documents(
    _auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    axum::extract::Path(context_id): axum::extract::Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    let (base, auth) = resolve_graphrag(&state.pool)
        .await
        .ok_or_else(|| AppError::NotFound("GraphRAG server not configured".to_string()))?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| AppError::Internal(format!("HTTP client error: {e}")))?;

    let resp = client
        .get(format!("{}/api/contexts/{}/documents", base, context_id))
        .header("Authorization", &auth)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("GraphRAG request failed: {e}")))?;

    let status = resp.status().as_u16();
    let data: serde_json::Value = resp.json().await.unwrap_or_default();

    if status != 200 {
        return Err(AppError::Internal(format!("GraphRAG documents error: HTTP {status}")));
    }

    Ok(Json(data))
}
