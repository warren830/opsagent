use std::sync::Arc;

use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::AppState;
use crate::error::{AppError, AppResult};
use crate::middleware::auth::AuthUser;
use crate::models::issue::Issue;

#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    #[allow(dead_code)]
    pub jsonrpc: Option<String>,
    pub id: Option<serde_json::Value>,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
}

impl JsonRpcResponse {
    fn success(id: Option<serde_json::Value>, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    fn error(id: Option<serde_json::Value>, code: i32, message: String) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError { code, message }),
        }
    }
}

fn tools_list() -> serde_json::Value {
    serde_json::json!({
        "tools": [
            {
                "name": "rca_get_status",
                "description": "Get the current RCA execution status for an issue. Returns whether an RCA is running, and if completed, the analysis result.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "issue_id": {
                            "type": "string",
                            "description": "UUID of the issue"
                        }
                    },
                    "required": ["issue_id"]
                }
            },
            {
                "name": "rca_start",
                "description": "Trigger a new RCA analysis for an issue. Returns immediately with started or already_running status. The analysis runs asynchronously.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "issue_id": {
                            "type": "string",
                            "description": "UUID of the issue"
                        }
                    },
                    "required": ["issue_id"]
                }
            },
            {
                "name": "rca_list_recent",
                "description": "List the 10 most recent issues that have RCA results or are currently being analyzed.",
                "inputSchema": {
                    "type": "object",
                    "properties": {},
                    "required": []
                }
            }
        ]
    })
}

pub async fn handle(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Json(req): Json<JsonRpcRequest>,
) -> AppResult<Json<JsonRpcResponse>> {
    match req.method.as_str() {
        "tools/list" => Ok(Json(JsonRpcResponse::success(req.id, tools_list()))),
        "tools/call" => {
            let tool_name = req.params.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let args = req
                .params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));

            match call_tool(&state, &auth_user, tool_name, &args).await {
                Ok(text) => Ok(Json(JsonRpcResponse::success(
                    req.id,
                    serde_json::json!({
                        "content": [{ "type": "text", "text": text }]
                    }),
                ))),
                Err(e) => Ok(Json(JsonRpcResponse::error(req.id, -32000, e.to_string()))),
            }
        }
        "initialize" => Ok(Json(JsonRpcResponse::success(
            req.id,
            serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "serverInfo": {
                    "name": "ops-rca",
                    "version": "1.0.0"
                }
            }),
        ))),
        _ => Ok(Json(JsonRpcResponse::error(
            req.id,
            -32601,
            format!("Method not found: {}", req.method),
        ))),
    }
}

async fn call_tool(
    state: &AppState,
    auth_user: &AuthUser,
    tool_name: &str,
    args: &serde_json::Value,
) -> Result<String, AppError> {
    match tool_name {
        "rca_get_status" => {
            let issue_id = parse_uuid(args, "issue_id")?;
            let issue = fetch_issue(&state.pool, auth_user, issue_id).await?;
            let is_running = state.rca_registry.is_running(issue_id).await;

            let status = if is_running {
                "running"
            } else if issue.rca_completed_at.is_some() {
                "completed"
            } else if issue.rca_started_at.is_some() {
                "failed"
            } else {
                "not_started"
            };

            let result = serde_json::json!({
                "issue_id": issue_id,
                "title": issue.title,
                "status": status,
                "rca_started_at": issue.rca_started_at,
                "rca_completed_at": issue.rca_completed_at,
                "rca_result": issue.rca_result,
            });

            Ok(serde_json::to_string_pretty(&result).unwrap_or_default())
        }

        "rca_start" => {
            let issue_id = parse_uuid(args, "issue_id")?;
            let issue = fetch_issue(&state.pool, auth_user, issue_id).await?;

            if state.rca_registry.is_running(issue_id).await {
                let result = serde_json::json!({
                    "issue_id": issue_id,
                    "status": "already_running",
                    "message": "RCA is already in progress for this issue"
                });
                return Ok(serde_json::to_string_pretty(&result).unwrap_or_default());
            }

            let pool_clone = state.pool.clone();
            let config_clone = Arc::new(state.config.clone());
            let registry_clone = state.rca_registry.clone();

            tokio::spawn(async move {
                crate::services::rca::run_rca(pool_clone, config_clone, registry_clone, issue).await;
            });

            let result = serde_json::json!({
                "issue_id": issue_id,
                "status": "started",
                "message": "RCA analysis has been triggered"
            });

            Ok(serde_json::to_string_pretty(&result).unwrap_or_default())
        }

        "rca_list_recent" => {
            let rows = if auth_user.is_super_admin() {
                sqlx::query_as::<_, Issue>(
                    r#"SELECT * FROM issues
                       WHERE rca_started_at IS NOT NULL
                       ORDER BY COALESCE(rca_completed_at, rca_started_at) DESC NULLS LAST
                       LIMIT 10"#,
                )
                .fetch_all(&state.pool)
                .await
                .map_err(|e| AppError::Internal(format!("DB: {e}")))?
            } else {
                sqlx::query_as::<_, Issue>(
                    r#"SELECT * FROM issues
                       WHERE rca_started_at IS NOT NULL
                         AND (tenant_id IS NOT DISTINCT FROM $1)
                       ORDER BY COALESCE(rca_completed_at, rca_started_at) DESC NULLS LAST
                       LIMIT 10"#,
                )
                .bind(auth_user.tenant_id)
                .fetch_all(&state.pool)
                .await
                .map_err(|e| AppError::Internal(format!("DB: {e}")))?
            };

            let items: Vec<serde_json::Value> = rows
                .iter()
                .map(|issue| {
                    let status = if issue.rca_completed_at.is_some() {
                        "completed"
                    } else {
                        "in_progress"
                    };
                    serde_json::json!({
                        "issue_id": issue.id,
                        "title": issue.title,
                        "severity": issue.severity,
                        "rca_status": status,
                        "rca_started_at": issue.rca_started_at,
                        "rca_completed_at": issue.rca_completed_at,
                    })
                })
                .collect();

            Ok(serde_json::to_string_pretty(&items).unwrap_or_default())
        }

        _ => Err(AppError::BadRequest(format!("Unknown tool: {tool_name}"))),
    }
}

fn parse_uuid(args: &serde_json::Value, key: &str) -> Result<Uuid, AppError> {
    args.get(key)
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| AppError::BadRequest(format!("{key} must be a UUID string")))
}

async fn fetch_issue(
    pool: &sqlx::PgPool,
    auth_user: &AuthUser,
    id: Uuid,
) -> Result<Issue, AppError> {
    let row = sqlx::query_as::<_, Issue>("SELECT * FROM issues WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|e| AppError::Internal(format!("DB: {e}")))?
        .ok_or_else(|| AppError::NotFound("Issue not found".to_string()))?;

    if !auth_user.is_super_admin() && row.tenant_id != auth_user.tenant_id {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }

    Ok(row)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tools_list_advertises_three_tools() {
        let v = tools_list();
        let arr = v.get("tools").and_then(|t| t.as_array()).unwrap();
        assert_eq!(arr.len(), 3);
        let names: Vec<&str> = arr
            .iter()
            .filter_map(|t| t.get("name").and_then(|n| n.as_str()))
            .collect();
        assert!(names.contains(&"rca_get_status"));
        assert!(names.contains(&"rca_start"));
        assert!(names.contains(&"rca_list_recent"));
    }

    #[test]
    fn parse_uuid_rejects_non_uuid() {
        let v = serde_json::json!({ "issue_id": "not-a-uuid" });
        assert!(parse_uuid(&v, "issue_id").is_err());
    }

    #[test]
    fn parse_uuid_accepts_valid_uuid() {
        let id = Uuid::new_v4();
        let v = serde_json::json!({ "issue_id": id.to_string() });
        assert_eq!(parse_uuid(&v, "issue_id").unwrap(), id);
    }
}
