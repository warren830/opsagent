//! MCP JSON-RPC bridge for incident tools — mirrors `mcp_rollout.rs`.
//!
//! Exposes three read-write tools so the agent copilot can participate in
//! a live incident without holding the IC's keyboard:
//!
//! - `incident.post_timeline_note` — drop a summary onto the timeline
//! - `incident.suggest_severity_change` — record a *suggestion* (does NOT
//!   change `incidents.severity`; the IC must confirm via the UI)
//! - `incident.propose_update_draft` — stage a stakeholder update as a
//!   draft in `incident_updates` (published_at NULL) so the IC can edit
//!   and publish.
//!
//! Tool design follows the "agent proposes, IC approves" rule from
//! platform-evolution.md §6.2. Writing a suggestion to the timeline is
//! always safe — it never bypasses the state machine or the severity
//! audit log.

use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::AppState;
use crate::error::{AppError, AppResult};
use crate::middleware::auth::AuthUser;
use crate::models::incident::{self, Incident, IncidentTimelineEvent, IncidentUpdate};
use crate::services::incident::timeline;

// ─── JSON-RPC envelope types (same shape as mcp_rollout.rs) ────────────────

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

// ─── Tool catalog ──────────────────────────────────────────────────────────

fn tools_list() -> serde_json::Value {
    serde_json::json!({
        "tools": [
            {
                "name": "incident.post_timeline_note",
                "description": "Append a free-form note to an incident timeline. The event is attributed to the AI agent (actor.kind = 'agent') so operators can tell human notes apart.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "incident_id": { "type": "string", "description": "UUID of the incident" },
                        "summary": { "type": "string", "description": "Short human-readable summary" },
                        "payload": { "type": "object", "description": "Optional structured data" }
                    },
                    "required": ["incident_id", "summary"]
                }
            },
            {
                "name": "incident.suggest_severity_change",
                "description": "Record a severity-change SUGGESTION in the timeline. Does NOT modify incidents.severity — the IC must confirm via the UI.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "incident_id": { "type": "string" },
                        "to_severity": { "type": "string", "description": "sev1|sev2|sev3|sev4" },
                        "reason": { "type": "string" }
                    },
                    "required": ["incident_id", "to_severity", "reason"]
                }
            },
            {
                "name": "incident.propose_update_draft",
                "description": "Create a draft stakeholder update (published_at = null). The IC edits + publishes via the UI.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "incident_id": { "type": "string" },
                        "audience": { "type": "string", "description": "internal|customers|stakeholders|status_page" },
                        "body_markdown": { "type": "string" }
                    },
                    "required": ["incident_id", "audience", "body_markdown"]
                }
            }
        ]
    })
}

// ─── Dispatch ──────────────────────────────────────────────────────────────

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
                "serverInfo": { "name": "ops-incidents", "version": "1.0.0" }
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
    let incident_id = parse_uuid(args, "incident_id")?;
    // Ensure caller can access this incident (tenant check).
    let inc = fetch_incident(&state.pool, auth_user, incident_id).await?;

    // Every tool call is attributed to the agent, not to the user.
    // `session_id` is opaque here — the chat handler stamps it into
    // timeline events separately via `maybe_record_agent_timeline`.
    let session_id = args
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("mcp-incident");
    let actor = timeline::agent_actor("claude", session_id);

    match tool_name {
        "incident.post_timeline_note" => {
            let summary = arg_str(args, "summary")?;
            if summary.trim().is_empty() {
                return Err(AppError::BadRequest("summary is required".to_string()));
            }
            let payload = args.get("payload").cloned().unwrap_or(serde_json::json!({}));
            let row = insert_timeline_row(
                &state.pool,
                incident_id,
                "manual_note",
                actor,
                summary,
                payload,
            )
            .await?;
            publish_event(state, incident_id, row.clone());
            Ok(format!("note recorded as event {}", row.id))
        }
        "incident.suggest_severity_change" => {
            let to_severity = arg_str(args, "to_severity")?;
            if !Incident::is_valid_severity(to_severity) {
                return Err(AppError::BadRequest(format!(
                    "invalid severity: {to_severity}"
                )));
            }
            let reason = arg_str(args, "reason")?;
            if reason.trim().is_empty() {
                return Err(AppError::BadRequest("reason is required".to_string()));
            }
            let summary = format!(
                "Agent suggests severity: {} → {} (requires IC approval)",
                inc.severity, to_severity
            );
            let payload = serde_json::json!({
                "from": inc.severity,
                "to": to_severity,
                "reason": reason,
                "is_suggestion": true,
            });
            let row = insert_timeline_row(
                &state.pool,
                incident_id,
                timeline::KIND_SEVERITY_CHANGED,
                actor,
                &summary,
                payload,
            )
            .await?;
            publish_event(state, incident_id, row.clone());
            Ok(summary)
        }
        "incident.propose_update_draft" => {
            let audience = arg_str(args, "audience")?;
            if !incident::ALL_UPDATE_AUDIENCES.contains(&audience) {
                return Err(AppError::BadRequest(format!(
                    "invalid audience: {audience}"
                )));
            }
            let body = arg_str(args, "body_markdown")?;
            if body.trim().is_empty() {
                return Err(AppError::BadRequest(
                    "body_markdown is required".to_string(),
                ));
            }

            // P3 #25: stamp the agent session id into `pushed_to` so the
            // UI can later show which chat produced this draft. No schema
            // change required — we reuse the JSONB column we already have.
            let pushed_to = if session_id == "mcp-incident" {
                // fallback sentinel — nothing to record
                serde_json::json!({})
            } else {
                serde_json::json!({ "agent_session_id": session_id })
            };

            let update = sqlx::query_as::<_, IncidentUpdate>(
                r#"INSERT INTO incident_updates
                       (incident_id, author_user_id, audience, status_at_time,
                        body_markdown, published_at, pushed_to)
                   VALUES ($1, NULL, $2, $3, $4, NULL, $5)
                   RETURNING *"#,
            )
            .bind(incident_id)
            .bind(audience)
            .bind(&inc.status)
            .bind(body.trim())
            .bind(&pushed_to)
            .fetch_one(&state.pool)
            .await?;

            // Note in timeline too so the IC notices a draft arrived.
            let summary = format!("Agent drafted stakeholder update for `{audience}`");
            let payload = serde_json::json!({
                "update_id": update.id,
                "audience": audience,
                "length": body.chars().count(),
            });
            let row = insert_timeline_row(
                &state.pool,
                incident_id,
                "update_drafted",
                actor,
                &summary,
                payload,
            )
            .await?;
            publish_event(state, incident_id, row);

            Ok(format!("draft {} staged for {}", update.id, audience))
        }
        _ => Err(AppError::BadRequest(format!("Unknown tool: {tool_name}"))),
    }
}

// ─── Helpers ───────────────────────────────────────────────────────────────

fn parse_uuid(args: &serde_json::Value, key: &str) -> Result<Uuid, AppError> {
    args.get(key)
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| AppError::BadRequest(format!("{key} must be a UUID string")))
}

fn arg_str<'a>(args: &'a serde_json::Value, key: &str) -> Result<&'a str, AppError> {
    args.get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::BadRequest(format!("{key} is required")))
}

async fn fetch_incident(
    pool: &PgPool,
    auth_user: &AuthUser,
    id: Uuid,
) -> Result<Incident, AppError> {
    let row = sqlx::query_as::<_, Incident>("SELECT * FROM incidents WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Incident not found".to_string()))?;
    if !auth_user.is_super_admin() && row.tenant_id != auth_user.tenant_id {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }
    Ok(row)
}

async fn insert_timeline_row(
    pool: &PgPool,
    incident_id: Uuid,
    kind: &str,
    actor: serde_json::Value,
    summary: &str,
    payload: serde_json::Value,
) -> Result<IncidentTimelineEvent, AppError> {
    let row = sqlx::query_as::<_, IncidentTimelineEvent>(
        r#"INSERT INTO incident_timeline_events
               (incident_id, kind, actor, summary, payload)
           VALUES ($1, $2, $3, $4, $5)
           RETURNING *"#,
    )
    .bind(incident_id)
    .bind(kind)
    .bind(&actor)
    .bind(summary)
    .bind(&payload)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

fn publish_event(state: &AppState, incident_id: Uuid, event: IncidentTimelineEvent) {
    state
        .timeline_bus
        .publish(crate::services::incident::timeline_bus::TimelineBroadcast {
            incident_id,
            event,
        });
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
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
        assert!(names.contains(&"incident.post_timeline_note"));
        assert!(names.contains(&"incident.suggest_severity_change"));
        assert!(names.contains(&"incident.propose_update_draft"));
    }

    #[test]
    fn parse_uuid_rejects_non_uuid() {
        let v = serde_json::json!({ "incident_id": "not-a-uuid" });
        assert!(parse_uuid(&v, "incident_id").is_err());
    }

    #[test]
    fn parse_uuid_accepts_valid_uuid() {
        let id = Uuid::new_v4();
        let v = serde_json::json!({ "incident_id": id.to_string() });
        assert_eq!(parse_uuid(&v, "incident_id").unwrap(), id);
    }
}
