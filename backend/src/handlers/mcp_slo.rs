//! MCP JSON-RPC bridge for the SLO engine (W6).
//!
//! Exposes two tools to the AI agent:
//!
//! * `slo_query`    — look up active SLOs for a component or by id, and
//!                    return the most recent budget / burn-rate snapshot.
//! * `slo_forecast` — naive linear forecast of when the current burn rate
//!                    will deplete the remaining error budget.
//!
//! JSON-RPC framing matches `mcp_rollout.rs`; keep the shape compatible so
//! the agent side can reuse its transport code.

use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::AppState;
use crate::error::{AppError, AppResult};
use crate::middleware::auth::AuthUser;
use crate::models::slo::{ErrorBudgetSnapshot, Slo};
use crate::services::slo::budget_calc;

// ─── JSON-RPC types (mirrors mcp_rollout.rs) ────────────────────────────────

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

// ─── Tool schema ─────────────────────────────────────────────────────────────

fn tools_list() -> serde_json::Value {
    json!({
        "tools": [
            {
                "name": "slo_query",
                "description": "List active SLOs (optionally filtered by component_id or slo_id). Each result carries the most recent SLI / budget / burn-rate snapshot plus the linked component's name so the agent can narrate context in RCA reports.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "component_id": { "type": "string", "description": "Catalog entity UUID to filter SLOs by; omit to return all SLOs visible to the caller." },
                        "slo_id":       { "type": "string", "description": "Return just this SLO (mutually exclusive with component_id)." },
                        "include_disabled": { "type": "boolean", "description": "Include SLOs with enabled=false (default false)." }
                    }
                }
            },
            {
                "name": "slo_forecast",
                "description": "Estimate when the current error budget will be depleted given the most recent 1h burn rate. Returns null/unknown when burn rate is zero or budget is already exhausted.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "slo_id": { "type": "string", "description": "Target SLO UUID." }
                    },
                    "required": ["slo_id"]
                }
            }
        ]
    })
}

// ─── HTTP entry — POST /api/mcp/slo ─────────────────────────────────────────

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
                .unwrap_or_else(|| json!({}));

            let result = match tool_name {
                "slo_query" => slo_query(&state, &auth_user, &arguments).await,
                "slo_forecast" => slo_forecast(&state, &auth_user, &arguments).await,
                _ => Err(AppError::BadRequest(format!("Unknown tool: {tool_name}"))),
            };

            match result {
                Ok(content) => Ok(Json(JsonRpcResponse::success(
                    req.id,
                    json!({
                        "content": [{ "type": "text", "text": content }]
                    }),
                ))),
                Err(e) => Ok(Json(JsonRpcResponse::error(req.id, -32000, e.to_string()))),
            }
        }
        "initialize" => Ok(Json(JsonRpcResponse::success(
            req.id,
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "serverInfo": {
                    "name": "ops-slo",
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

// ─── slo_query ───────────────────────────────────────────────────────────────

async fn slo_query(
    state: &AppState,
    auth_user: &AuthUser,
    args: &serde_json::Value,
) -> Result<String, AppError> {
    let component_id = parse_opt_uuid(args, "component_id")?;
    let slo_id = parse_opt_uuid(args, "slo_id")?;
    let include_disabled = args
        .get("include_disabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if component_id.is_some() && slo_id.is_some() {
        return Err(AppError::BadRequest(
            "component_id and slo_id are mutually exclusive".to_string(),
        ));
    }

    let slos = fetch_visible_slos(state, auth_user, slo_id, component_id, include_disabled).await?;

    let mut out = Vec::with_capacity(slos.len());
    for slo in slos {
        let snap = fetch_latest_snapshot(state, slo.id).await?;
        let component_name = match slo.component_id {
            Some(cid) => fetch_component_name(state, cid).await?,
            None => None,
        };

        out.push(json!({
            "id": slo.id,
            "name": slo.name,
            "objective_pct": slo.objective_pct,
            "window_days": slo.window_days,
            "sli_type": slo.sli_type,
            "enabled": slo.enabled,
            "component_id": slo.component_id,
            "component_name": component_name,
            "current_sli_pct":          snap.as_ref().map(|s| s.sli_achieved_pct),
            "current_budget_remaining_pct": snap.as_ref().map(|s| s.budget_remaining_pct),
            "current_budget_consumed_minutes": snap.as_ref().map(|s| s.budget_consumed_minutes),
            "current_budget_total_minutes":    snap.as_ref().map(|s| s.budget_total_minutes),
            "current_burn_rate_1h":  snap.as_ref().and_then(|s| s.burn_rate_1h),
            "current_burn_rate_6h":  snap.as_ref().and_then(|s| s.burn_rate_6h),
            "current_burn_rate_24h": snap.as_ref().and_then(|s| s.burn_rate_24h),
            "current_burn_rate_3d":  snap.as_ref().and_then(|s| s.burn_rate_3d),
            "snapshot_captured_at":  snap.as_ref().map(|s| s.captured_at),
        }));
    }

    Ok(serde_json::to_string_pretty(&json!({ "slos": out })).unwrap_or_default())
}

// ─── slo_forecast ────────────────────────────────────────────────────────────

async fn slo_forecast(
    state: &AppState,
    auth_user: &AuthUser,
    args: &serde_json::Value,
) -> Result<String, AppError> {
    let slo_id = parse_opt_uuid(args, "slo_id")?
        .ok_or_else(|| AppError::BadRequest("slo_id is required".to_string()))?;

    let slo = fetch_visible_slos(state, auth_user, Some(slo_id), None, true)
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| AppError::NotFound(format!("SLO {slo_id} not found")))?;

    let snap = fetch_latest_snapshot(state, slo.id)
        .await?
        .ok_or_else(|| AppError::NotFound("No budget snapshot available yet".to_string()))?;

    let burn = snap.burn_rate_1h.unwrap_or(0.0);
    let remaining_minutes =
        (snap.budget_total_minutes - snap.budget_consumed_minutes).max(0.0);
    let (hours_until_depletion, depletes_at, reasoning) =
        project_depletion(&slo, &snap, burn, remaining_minutes);

    let payload = json!({
        "slo_id": slo.id,
        "slo_name": slo.name,
        "objective_pct": slo.objective_pct,
        "current_burn_rate_1h": snap.burn_rate_1h,
        "current_budget_remaining_pct": snap.budget_remaining_pct,
        "current_budget_remaining_minutes": remaining_minutes,
        "hours_until_depletion": hours_until_depletion,
        "budget_depletes_at": depletes_at,
        "reasoning": reasoning,
    });

    Ok(serde_json::to_string_pretty(&payload).unwrap_or_default())
}

/// Pure forecast logic — split out so it's unit-testable without the DB.
///
/// Returns `(hours_until_depletion, ISO-8601 timestamp, human reasoning)`.
fn project_depletion(
    slo: &Slo,
    snap: &ErrorBudgetSnapshot,
    burn: f64,
    remaining_minutes: f64,
) -> (Option<f64>, Option<String>, String) {
    if !burn.is_finite() || burn <= 0.0 {
        return (
            None,
            None,
            "Current 1h burn rate is zero or unavailable — budget is not depleting.".to_string(),
        );
    }
    if remaining_minutes <= 0.0 {
        return (
            Some(0.0),
            Some(snap.captured_at.to_rfc3339()),
            format!(
                "Error budget already exhausted (remaining={:.2} min, burn={:.2}x).",
                remaining_minutes, burn
            ),
        );
    }

    // The canonical Google SRE formula: burn_rate = consumed_rate / allowed_rate.
    // allowed_rate (minutes/hour) = budget_total / (window_days * 24).
    let allowed_per_hour = budget_calc::total_minutes(slo.objective_pct, slo.window_days)
        / (slo.window_days as f64 * 24.0);
    let consumed_per_hour = (burn * allowed_per_hour).max(0.0);
    if consumed_per_hour <= 0.0 {
        return (
            None,
            None,
            "Unable to project depletion — consumption rate is zero.".to_string(),
        );
    }

    let hours = remaining_minutes / consumed_per_hour;
    let depletes_at = snap.captured_at + chrono::Duration::seconds((hours * 3600.0) as i64);
    let reasoning = format!(
        "Burning {:.2}x ({:.2} budget-min/h). {:.2} min of budget remain → ~{:.1}h until depletion.",
        burn, consumed_per_hour, remaining_minutes, hours
    );
    (Some(hours), Some(depletes_at.to_rfc3339()), reasoning)
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn parse_opt_uuid(args: &serde_json::Value, field: &str) -> Result<Option<Uuid>, AppError> {
    match args.get(field).and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => Uuid::parse_str(s)
            .map(Some)
            .map_err(|_| AppError::BadRequest(format!("{field} must be a valid UUID"))),
        _ => Ok(None),
    }
}

async fn fetch_visible_slos(
    state: &AppState,
    auth_user: &AuthUser,
    slo_id: Option<Uuid>,
    component_id: Option<Uuid>,
    include_disabled: bool,
) -> AppResult<Vec<Slo>> {
    // Tenant scoping mirrors the REST list handler: super_admin sees all,
    // everyone else is bound to their tenant_id. A missing tenant context
    // (super_admin with no tenant) still sees everything.
    let tenant_filter = if auth_user.is_super_admin() {
        None
    } else {
        Some(
            auth_user
                .tenant_id
                .ok_or_else(|| AppError::Forbidden("No tenant context".to_string()))?,
        )
    };

    let rows = sqlx::query_as::<_, Slo>(
        r#"SELECT * FROM slos
           WHERE ($1::uuid IS NULL OR tenant_id = $1)
             AND ($2::uuid IS NULL OR id = $2)
             AND ($3::uuid IS NULL OR component_id = $3)
             AND ($4 OR enabled = TRUE)
           ORDER BY created_at DESC"#,
    )
    .bind(tenant_filter)
    .bind(slo_id)
    .bind(component_id)
    .bind(include_disabled)
    .fetch_all(&state.pool)
    .await?;
    Ok(rows)
}

async fn fetch_latest_snapshot(
    state: &AppState,
    slo_id: Uuid,
) -> AppResult<Option<ErrorBudgetSnapshot>> {
    let row = sqlx::query_as::<_, ErrorBudgetSnapshot>(
        r#"SELECT * FROM error_budget_snapshots
           WHERE slo_id = $1
           ORDER BY captured_at DESC
           LIMIT 1"#,
    )
    .bind(slo_id)
    .fetch_optional(&state.pool)
    .await?;
    Ok(row)
}

async fn fetch_component_name(state: &AppState, component_id: Uuid) -> AppResult<Option<String>> {
    let row: Option<(Option<String>, String)> = sqlx::query_as(
        "SELECT display_name, name FROM catalog_entities WHERE id = $1 LIMIT 1",
    )
    .bind(component_id)
    .fetch_optional(&state.pool)
    .await?;
    Ok(row.map(|(display, name)| display.unwrap_or(name)))
}

// ─── Tests (pure forecast math only — DB paths are integration-tested) ──────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn make_slo() -> Slo {
        Slo {
            id: Uuid::nil(),
            tenant_id: Uuid::nil(),
            component_id: None,
            name: "test".into(),
            description: None,
            sli_type: "availability".into(),
            good_events_query: "good".into(),
            total_events_query: "total".into(),
            objective_pct: 99.9,
            window_days: 28,
            burn_rate_policy: "mwmbr_default".into(),
            labels: serde_json::json!({}),
            enabled: true,
            recording_rules_hash: None,
            created_by: Uuid::nil(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn make_snap(remaining: f64, burn: Option<f64>) -> ErrorBudgetSnapshot {
        let total = 40.32; // 99.9 over 28d
        ErrorBudgetSnapshot {
            id: Uuid::nil(),
            slo_id: Uuid::nil(),
            captured_at: Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
            window_start: Utc.timestamp_opt(1_700_000_000 - 28 * 86400, 0).unwrap(),
            window_end: Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
            sli_achieved_pct: 99.95,
            budget_total_minutes: total,
            budget_consumed_minutes: total - remaining,
            budget_remaining_pct: remaining / total * 100.0,
            burn_rate_1h: burn,
            burn_rate_6h: None,
            burn_rate_24h: None,
            burn_rate_3d: None,
        }
    }

    #[test]
    fn project_depletion_zero_burn_returns_none() {
        let slo = make_slo();
        let snap = make_snap(20.0, Some(0.0));
        let (hours, at, _) = project_depletion(&slo, &snap, 0.0, 20.0);
        assert!(hours.is_none());
        assert!(at.is_none());
    }

    #[test]
    fn project_depletion_already_exhausted_returns_zero() {
        let slo = make_slo();
        let snap = make_snap(0.0, Some(5.0));
        let (hours, at, _) = project_depletion(&slo, &snap, 5.0, 0.0);
        assert_eq!(hours, Some(0.0));
        assert!(at.is_some());
    }

    #[test]
    fn project_depletion_fourteen_x_burn_drains_in_expected_time() {
        // SLO 99.9 / 28d → total 40.32 min, allowed_per_hour = 40.32 / 672.
        // burn=14.4 → consumed_per_hour ≈ 0.864 min/h.
        // 20 min remaining → ≈ 23.15h.
        let slo = make_slo();
        let snap = make_snap(20.0, Some(14.4));
        let (hours, _, reasoning) = project_depletion(&slo, &snap, 14.4, 20.0);
        let h = hours.expect("forecast should project a value");
        assert!(h > 20.0 && h < 30.0, "expected ~23h, got {h}");
        assert!(reasoning.contains("14.40x") || reasoning.contains("14.4"));
    }

    #[test]
    fn project_depletion_nonfinite_burn_returns_none() {
        let slo = make_slo();
        let snap = make_snap(20.0, None);
        let (hours, _, _) = project_depletion(&slo, &snap, f64::NAN, 20.0);
        assert!(hours.is_none());
    }
}
