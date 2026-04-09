use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::error::{AppError, AppResult};
use crate::middleware::auth::AuthUser;
use crate::models::rollout::{parse_analysis_run, parse_canary_steps, parse_containers, parse_rollout_summary};
use crate::services::k8s::build_k8s_client;
use kube::api::{Api, DynamicObject, Patch, PatchParams};
use kube::discovery::ApiResource;

// ─── JSON-RPC types ──────────────────────────────────────────────────────────

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

// ─── MCP tool definitions ────────────────────────────────────────────────────

fn tools_list() -> serde_json::Value {
    serde_json::json!({
        "tools": [
            {
                "name": "list_rollouts",
                "description": "List all Argo Rollouts across all namespaces in a cluster. Shows status, strategy, replica counts, and canary weight.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "cluster_name": {
                            "type": "string",
                            "description": "Name of the EKS cluster"
                        }
                    },
                    "required": ["cluster_name"]
                }
            },
            {
                "name": "get_rollout_detail",
                "description": "Get detailed info for a specific Rollout including canary steps, container images, and analysis results.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "cluster_name": { "type": "string", "description": "Name of the EKS cluster" },
                        "namespace": { "type": "string", "description": "Kubernetes namespace" },
                        "name": { "type": "string", "description": "Rollout name" }
                    },
                    "required": ["cluster_name", "namespace", "name"]
                }
            },
            {
                "name": "promote_rollout",
                "description": "Promote a paused canary rollout. Use full=false to advance one step, full=true to promote to 100% traffic.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "cluster_name": { "type": "string", "description": "Name of the EKS cluster" },
                        "namespace": { "type": "string", "description": "Kubernetes namespace" },
                        "name": { "type": "string", "description": "Rollout name" },
                        "full": { "type": "boolean", "description": "true = full promotion, false = advance one step", "default": false }
                    },
                    "required": ["cluster_name", "namespace", "name"]
                }
            },
            {
                "name": "rollback_rollout",
                "description": "Abort and rollback a Rollout to the previous stable version.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "cluster_name": { "type": "string", "description": "Name of the EKS cluster" },
                        "namespace": { "type": "string", "description": "Kubernetes namespace" },
                        "name": { "type": "string", "description": "Rollout name" }
                    },
                    "required": ["cluster_name", "namespace", "name"]
                }
            }
        ]
    })
}

// ─── POST /api/mcp/rollouts — JSON-RPC handler ──────────────────────────────

pub async fn handle(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Json(req): Json<JsonRpcRequest>,
) -> AppResult<Json<JsonRpcResponse>> {
    match req.method.as_str() {
        "tools/list" => Ok(Json(JsonRpcResponse::success(req.id, tools_list()))),
        "tools/call" => {
            let tool_name = req.params.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let arguments = req
                .params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));

            let result = call_tool(&state, &auth_user, tool_name, &arguments).await;

            match result {
                Ok(content) => Ok(Json(JsonRpcResponse::success(
                    req.id,
                    serde_json::json!({
                        "content": [{ "type": "text", "text": content }]
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
                    "name": "openops-rollouts",
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

// ─── Tool dispatch ───────────────────────────────────────────────────────────

fn rollout_ar() -> ApiResource {
    ApiResource {
        group: "argoproj.io".to_string(),
        version: "v1alpha1".to_string(),
        api_version: "argoproj.io/v1alpha1".to_string(),
        kind: "Rollout".to_string(),
        plural: "rollouts".to_string(),
    }
}

fn analysis_ar() -> ApiResource {
    ApiResource {
        group: "argoproj.io".to_string(),
        version: "v1alpha1".to_string(),
        api_version: "argoproj.io/v1alpha1".to_string(),
        kind: "AnalysisRun".to_string(),
        plural: "analysisruns".to_string(),
    }
}

async fn call_tool(
    state: &AppState,
    auth_user: &AuthUser,
    tool_name: &str,
    args: &serde_json::Value,
) -> Result<String, AppError> {
    let cluster_name = args
        .get("cluster_name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::BadRequest("cluster_name is required".to_string()))?;

    // Resolve cluster by name
    let cluster = resolve_cluster(&state.pool, auth_user, cluster_name).await?;
    let client = build_k8s_client(&state.pool, &cluster).await?;

    match tool_name {
        "list_rollouts" => {
            let api: Api<DynamicObject> = Api::all_with(client, &rollout_ar());
            let list = api
                .list(&Default::default())
                .await
                .map_err(|e| AppError::Kubernetes(format!("List rollouts: {e}")))?;

            let rollouts: Vec<_> = list
                .items
                .iter()
                .filter_map(|obj| {
                    let raw = serde_json::to_value(obj).ok()?;
                    parse_rollout_summary(&raw)
                })
                .collect();

            Ok(serde_json::to_string_pretty(&rollouts).unwrap_or_default())
        }

        "get_rollout_detail" => {
            let ns = args
                .get("namespace")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::BadRequest("namespace is required".to_string()))?;
            let name = args
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::BadRequest("name is required".to_string()))?;

            let api: Api<DynamicObject> = Api::namespaced_with(client.clone(), ns, &rollout_ar());
            let obj = api
                .get(name)
                .await
                .map_err(|e| AppError::Kubernetes(format!("Get rollout: {e}")))?;

            let raw = serde_json::to_value(&obj).map_err(|e| AppError::Internal(format!("Serialize: {e}")))?;

            let summary =
                parse_rollout_summary(&raw).ok_or_else(|| AppError::Internal("Parse rollout failed".to_string()))?;
            let steps = parse_canary_steps(&raw, summary.current_step);
            let containers = parse_containers(&raw);

            // Also fetch analysis runs
            let analysis_api: Api<DynamicObject> = Api::namespaced_with(client, ns, &analysis_ar());
            let ar_list = analysis_api.list(&Default::default()).await.ok();
            let analysis_runs: Vec<_> = ar_list
                .map(|list| {
                    list.items
                        .iter()
                        .filter(|o| {
                            o.metadata
                                .owner_references
                                .as_ref()
                                .map(|refs| refs.iter().any(|r| r.name == name && r.kind == "Rollout"))
                                .unwrap_or(false)
                        })
                        .filter_map(|o| {
                            let raw = serde_json::to_value(o).ok()?;
                            parse_analysis_run(&raw)
                        })
                        .collect()
                })
                .unwrap_or_default();

            let detail = serde_json::json!({
                "summary": summary,
                "canary_steps": steps,
                "containers": containers,
                "analysis_runs": analysis_runs,
            });

            Ok(serde_json::to_string_pretty(&detail).unwrap_or_default())
        }

        "promote_rollout" => {
            let ns = args
                .get("namespace")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::BadRequest("namespace required".to_string()))?;
            let name = args
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::BadRequest("name required".to_string()))?;
            let full = args.get("full").and_then(|v| v.as_bool()).unwrap_or(false);

            // Check write permission
            check_write_for_mcp(&state.pool, auth_user, &cluster).await?;

            let api: Api<DynamicObject> = Api::namespaced_with(client, ns, &rollout_ar());

            let patch = if full {
                serde_json::json!({
                    "status": {
                        "pauseConditions": null,
                        "controllerPause": false,
                        "promoteFull": true
                    }
                })
            } else {
                serde_json::json!({
                    "status": {
                        "pauseConditions": null,
                        "controllerPause": false
                    }
                })
            };

            api.patch_status(name, &PatchParams::default(), &Patch::Merge(&patch))
                .await
                .map_err(|e| AppError::Kubernetes(format!("Promote: {e}")))?;

            Ok(format!(
                "Successfully {} rollout {}/{}",
                if full { "fully promoted" } else { "advanced" },
                ns,
                name
            ))
        }

        "rollback_rollout" => {
            let ns = args
                .get("namespace")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::BadRequest("namespace required".to_string()))?;
            let name = args
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::BadRequest("name required".to_string()))?;

            check_write_for_mcp(&state.pool, auth_user, &cluster).await?;

            let api: Api<DynamicObject> = Api::namespaced_with(client, ns, &rollout_ar());
            let patch = serde_json::json!({ "status": { "abort": true } });

            api.patch_status(name, &PatchParams::default(), &Patch::Merge(&patch))
                .await
                .map_err(|e| AppError::Kubernetes(format!("Rollback: {e}")))?;

            Ok(format!("Successfully rolled back rollout {}/{}", ns, name))
        }

        _ => Err(AppError::BadRequest(format!("Unknown tool: {tool_name}"))),
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

async fn resolve_cluster(
    pool: &sqlx::PgPool,
    auth_user: &AuthUser,
    cluster_name: &str,
) -> Result<crate::models::cluster::Cluster, AppError> {
    let cluster = if auth_user.is_super_admin() {
        sqlx::query_as::<_, crate::models::cluster::Cluster>("SELECT * FROM clusters WHERE name = $1 LIMIT 1")
            .bind(cluster_name)
            .fetch_optional(pool)
            .await
            .map_err(|e| AppError::Internal(format!("DB: {e}")))?
    } else {
        sqlx::query_as::<_, crate::models::cluster::Cluster>(
            "SELECT * FROM clusters WHERE name = $1 AND tenant_id IS NOT DISTINCT FROM $2 LIMIT 1",
        )
        .bind(cluster_name)
        .bind(auth_user.tenant_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| AppError::Internal(format!("DB: {e}")))?
    };

    cluster.ok_or_else(|| AppError::NotFound(format!("Cluster '{}' not found", cluster_name)))
}

async fn check_write_for_mcp(
    pool: &sqlx::PgPool,
    auth_user: &AuthUser,
    cluster: &crate::models::cluster::Cluster,
) -> AppResult<()> {
    if auth_user.is_super_admin() {
        return Ok(());
    }

    if let Some(ref account_id) = cluster.account_id {
        let maybe_id: Option<uuid::Uuid> =
            sqlx::query_scalar("SELECT id FROM cloud_accounts WHERE account_id = $1 LIMIT 1")
                .bind(account_id)
                .fetch_optional(pool)
                .await
                .map_err(|e| AppError::Internal(format!("DB: {e}")))?;

        if let Some(internal_id) = maybe_id {
            let can_write: bool = sqlx::query_scalar(
                r#"SELECT EXISTS(
                    SELECT 1 FROM user_account_access
                    WHERE user_id = $1 AND account_id = $2 AND role = 'admin'
                )"#,
            )
            .bind(auth_user.user_id)
            .bind(internal_id)
            .fetch_one(pool)
            .await
            .unwrap_or(false);

            if !can_write {
                return Err(AppError::Forbidden(
                    "Read-only access: cannot modify rollouts".to_string(),
                ));
            }
        }
    }

    Ok(())
}
