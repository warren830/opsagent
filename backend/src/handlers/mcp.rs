use axum::{
    Json,
    extract::{Path, State},
};
use uuid::Uuid;

use crate::AppState;
use crate::error::{AppError, AppResult};
use crate::middleware::auth::AuthUser;
use crate::models::mcp::{CreateMcpServerRequest, McpServer, UpdateMcpServerRequest};

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
