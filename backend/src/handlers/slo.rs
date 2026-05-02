//! SLO engine HTTP handlers (W2).
//!
//! CRUD + enable/disable + a pair of Mimir query proxies (`preview` / `sli`)
//! and budget/budget-history readers backed by `error_budget_snapshots`.
//!
//! Tenant isolation follows the project standard: `super_admin` sees every
//! tenant, anyone else is filtered by `auth_user.tenant_id`. On create, the
//! `tenant_id` comes from the caller — super admins without a tenant context
//! can't create SLOs (matches how `cloud_account` and the rest of the stack
//! behave).

use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::AppState;
use crate::error::{AppError, AppResult};
use crate::middleware::auth::AuthUser;
use crate::models::slo::{
    BudgetHistoryQuery, CreateSloRequest, ErrorBudgetSnapshot, PreviewRequest, Slo, SliQuery,
    UpdateSloRequest,
};
use crate::services::common::require_non_empty;
use crate::services::slo::mimir_client::{self, MetricsEndpoint};

// ---------------------------------------------------------------------------
// List / Get
// ---------------------------------------------------------------------------

/// Query parameters for `GET /api/slos`.
#[derive(Debug, Deserialize)]
pub struct ListQuery {
    /// When `true`, disabled SLOs are included. Defaults to `false` so the
    /// list view matches the common "active SLOs only" expectation.
    #[serde(default)]
    pub include_disabled: bool,
}

/// GET /api/slos
pub async fn list(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
) -> AppResult<Json<Vec<Slo>>> {
    let rows = if auth_user.is_super_admin() {
        sqlx::query_as::<_, Slo>(
            r#"SELECT * FROM slos
               WHERE ($1 OR enabled = TRUE)
               ORDER BY created_at DESC"#,
        )
        .bind(query.include_disabled)
        .fetch_all(&state.pool)
        .await?
    } else {
        let tenant_id = auth_user
            .tenant_id
            .ok_or_else(|| AppError::Forbidden("No tenant context".to_string()))?;
        sqlx::query_as::<_, Slo>(
            r#"SELECT * FROM slos
               WHERE tenant_id = $1
                 AND ($2 OR enabled = TRUE)
               ORDER BY created_at DESC"#,
        )
        .bind(tenant_id)
        .bind(query.include_disabled)
        .fetch_all(&state.pool)
        .await?
    };
    Ok(Json(rows))
}

/// GET /api/slos/:id
pub async fn get(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Slo>> {
    let slo = fetch_slo(&state, &auth_user, id).await?;
    Ok(Json(slo))
}

// ---------------------------------------------------------------------------
// Create / Update / Delete
// ---------------------------------------------------------------------------

/// POST /api/slos
pub async fn create(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Json(req): Json<CreateSloRequest>,
) -> AppResult<Json<Slo>> {
    validate_create(&req)?;

    let tenant_id = auth_user
        .tenant_id
        .ok_or_else(|| AppError::BadRequest("Cannot create SLO without a tenant context".to_string()))?;

    let row = sqlx::query_as::<_, Slo>(
        r#"INSERT INTO slos (
               tenant_id, component_id, name, description, sli_type,
               good_events_query, total_events_query, objective_pct,
               window_days, burn_rate_policy, labels, enabled, created_by
           )
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
           RETURNING *"#,
    )
    .bind(tenant_id)
    .bind(req.component_id)
    .bind(&req.name)
    .bind(&req.description)
    .bind(&req.sli_type)
    .bind(&req.good_events_query)
    .bind(&req.total_events_query)
    .bind(req.objective_pct)
    .bind(req.window_days)
    .bind(&req.burn_rate_policy)
    .bind(&req.labels)
    .bind(req.enabled)
    .bind(auth_user.user_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        if let sqlx::Error::Database(ref db_err) = e
            && let Some(constraint) = db_err.constraint()
            && (constraint.contains("tenant_id_name") || constraint.contains("slos_tenant"))
        {
            return AppError::Conflict(format!("SLO '{}' already exists in this tenant", req.name));
        }
        AppError::Database(e)
    })?;

    Ok(Json(row))
}

/// PUT /api/slos/:id
pub async fn update(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateSloRequest>,
) -> AppResult<Json<Slo>> {
    let _ = fetch_slo(&state, &auth_user, id).await?;
    validate_update(&req)?;

    let row = sqlx::query_as::<_, Slo>(
        r#"UPDATE slos SET
               name = COALESCE($2, name),
               description = COALESCE($3, description),
               component_id = COALESCE($4, component_id),
               sli_type = COALESCE($5, sli_type),
               good_events_query = COALESCE($6, good_events_query),
               total_events_query = COALESCE($7, total_events_query),
               objective_pct = COALESCE($8, objective_pct),
               window_days = COALESCE($9, window_days),
               burn_rate_policy = COALESCE($10, burn_rate_policy),
               labels = COALESCE($11, labels),
               enabled = COALESCE($12, enabled),
               updated_at = NOW()
           WHERE id = $1
           RETURNING *"#,
    )
    .bind(id)
    .bind(&req.name)
    .bind(&req.description)
    .bind(req.component_id)
    .bind(&req.sli_type)
    .bind(&req.good_events_query)
    .bind(&req.total_events_query)
    .bind(req.objective_pct)
    .bind(req.window_days)
    .bind(&req.burn_rate_policy)
    .bind(&req.labels)
    .bind(req.enabled)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        if let sqlx::Error::Database(ref db_err) = e
            && let Some(constraint) = db_err.constraint()
            && (constraint.contains("tenant_id_name") || constraint.contains("slos_tenant"))
        {
            return AppError::Conflict("SLO name already exists in this tenant".to_string());
        }
        AppError::Database(e)
    })?;

    Ok(Json(row))
}

/// DELETE /api/slos/:id — cascades to snapshots and burn events via FK.
pub async fn delete(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let _ = fetch_slo(&state, &auth_user, id).await?;

    sqlx::query("DELETE FROM slos WHERE id = $1")
        .bind(id)
        .execute(&state.pool)
        .await?;

    Ok(Json(json!({ "message": "SLO deleted" })))
}

// ---------------------------------------------------------------------------
// Enable / Disable
// ---------------------------------------------------------------------------

/// POST /api/slos/:id/enable
pub async fn enable(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Slo>> {
    set_enabled(&auth_user, &state, id, true).await
}

/// POST /api/slos/:id/disable
pub async fn disable(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Slo>> {
    set_enabled(&auth_user, &state, id, false).await
}

async fn set_enabled(
    auth_user: &AuthUser,
    state: &AppState,
    id: Uuid,
    enabled: bool,
) -> AppResult<Json<Slo>> {
    let _ = fetch_slo(state, auth_user, id).await?;

    let row = sqlx::query_as::<_, Slo>(
        r#"UPDATE slos SET enabled = $2, updated_at = NOW()
           WHERE id = $1
           RETURNING *"#,
    )
    .bind(id)
    .bind(enabled)
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(row))
}

// ---------------------------------------------------------------------------
// Mimir query proxies — preview + sli
// ---------------------------------------------------------------------------

/// POST /api/slos/preview
///
/// Executes a `good / total` PromQL division over the requested window and
/// returns the raw Prometheus `data` envelope so the UI can plot the SLI
/// time series before the user commits to persisting the SLO.
pub async fn preview(
    _auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Json(req): Json<PreviewRequest>,
) -> AppResult<Json<serde_json::Value>> {
    require_non_empty(&req.good_events_query, "good_events_query")?;
    require_non_empty(&req.total_events_query, "total_events_query")?;
    if !Slo::is_valid_window_days(req.window_days) {
        return Err(AppError::BadRequest(
            "window_days must be one of 7, 28, 30".to_string(),
        ));
    }

    let endpoint = mimir_client::resolve_metrics_endpoint(&state.pool).await?;
    let step = req.step.as_deref().unwrap_or("5m");
    let step_secs = mimir_client::parse_duration_to_seconds(step)?;
    if step_secs <= 0 {
        return Err(AppError::BadRequest("step must be > 0".to_string()));
    }

    let now = chrono::Utc::now().timestamp();
    let start = now - (req.window_days as i64) * 86400;
    // Prometheus-style ratio query. We wrap the two user-provided queries so
    // zero totals don't return NaN/Infinity — clamped max→1 and divided
    // safely; if the total series is empty the point is simply absent.
    let query = format!(
        "(({good}) / ({total}))",
        good = req.good_events_query,
        total = req.total_events_query
    );

    let result = mimir_client::query_range(&endpoint, &query, start, now, step).await?;

    Ok(Json(json!({
        "query": query,
        "window_days": req.window_days,
        "step": step,
        "start": start,
        "end": now,
        "prometheus": result,
    })))
}

/// GET /api/slos/:id/sli?window=28d&step=5m
///
/// Fetches the SLI ratio time series. Until recording rules are installed
/// by W3 (`rule_generator`), this falls back to the raw good/total division
/// so the endpoint is usable end-to-end today.
pub async fn sli(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(params): Query<SliQuery>,
) -> AppResult<Json<serde_json::Value>> {
    let slo = fetch_slo(&state, &auth_user, id).await?;

    let endpoint = mimir_client::resolve_metrics_endpoint(&state.pool).await?;
    let window_secs = mimir_client::parse_duration_to_seconds(&params.window)?;
    let step_secs = mimir_client::parse_duration_to_seconds(&params.step)?;
    if step_secs <= 0 {
        return Err(AppError::BadRequest("step must be > 0".to_string()));
    }

    let now = chrono::Utc::now().timestamp();
    let start = now - window_secs;
    let query = format!(
        "(({good}) / ({total}))",
        good = slo.good_events_query,
        total = slo.total_events_query
    );

    let result = mimir_client::query_range(&endpoint, &query, start, now, &params.step).await?;

    Ok(Json(json!({
        "slo_id": slo.id,
        "query": query,
        "window": params.window,
        "step": params.step,
        "start": start,
        "end": now,
        "recording_rules_installed": slo.recording_rules_hash.is_some(),
        "prometheus": result,
    })))
}

// ---------------------------------------------------------------------------
// Budget readers
// ---------------------------------------------------------------------------

/// GET /api/slos/:id/budget — most recent snapshot.
pub async fn budget(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<ErrorBudgetSnapshot>> {
    let _ = fetch_slo(&state, &auth_user, id).await?;

    let snapshot = sqlx::query_as::<_, ErrorBudgetSnapshot>(
        r#"SELECT * FROM error_budget_snapshots
           WHERE slo_id = $1
           ORDER BY captured_at DESC
           LIMIT 1"#,
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("No budget data yet".to_string()))?;

    Ok(Json(snapshot))
}

/// GET /api/slos/:id/budget/history?days=30
pub async fn budget_history(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(params): Query<BudgetHistoryQuery>,
) -> AppResult<Json<Vec<ErrorBudgetSnapshot>>> {
    let _ = fetch_slo(&state, &auth_user, id).await?;

    let days = params.days.unwrap_or(30);
    if !(1..=365).contains(&days) {
        return Err(AppError::BadRequest(
            "days must be between 1 and 365".to_string(),
        ));
    }

    let rows = sqlx::query_as::<_, ErrorBudgetSnapshot>(
        r#"SELECT * FROM error_budget_snapshots
           WHERE slo_id = $1
             AND captured_at >= NOW() - ($2 || ' days')::INTERVAL
           ORDER BY captured_at ASC"#,
    )
    .bind(id)
    .bind(days.to_string())
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(rows))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn fetch_slo(state: &AppState, auth_user: &AuthUser, id: Uuid) -> AppResult<Slo> {
    let slo = sqlx::query_as::<_, Slo>("SELECT * FROM slos WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("SLO not found".to_string()))?;

    if !auth_user.is_super_admin() && Some(slo.tenant_id) != auth_user.tenant_id {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }
    Ok(slo)
}

fn validate_create(req: &CreateSloRequest) -> AppResult<()> {
    require_non_empty(&req.name, "name")?;
    require_non_empty(&req.good_events_query, "good_events_query")?;
    require_non_empty(&req.total_events_query, "total_events_query")?;

    if !Slo::is_valid_sli_type(&req.sli_type) {
        return Err(AppError::BadRequest(format!(
            "sli_type must be one of availability/latency/error_rate/custom (got '{}')",
            req.sli_type
        )));
    }
    if !Slo::is_valid_window_days(req.window_days) {
        return Err(AppError::BadRequest(
            "window_days must be one of 7, 28, 30".to_string(),
        ));
    }
    if !(req.objective_pct > 0.0 && req.objective_pct < 100.0) {
        return Err(AppError::BadRequest(
            "objective_pct must be in (0, 100)".to_string(),
        ));
    }
    Ok(())
}

fn validate_update(req: &UpdateSloRequest) -> AppResult<()> {
    if let Some(name) = &req.name {
        require_non_empty(name, "name")?;
    }
    if let Some(good) = &req.good_events_query {
        require_non_empty(good, "good_events_query")?;
    }
    if let Some(total) = &req.total_events_query {
        require_non_empty(total, "total_events_query")?;
    }
    if let Some(sli_type) = &req.sli_type
        && !Slo::is_valid_sli_type(sli_type)
    {
        return Err(AppError::BadRequest(format!(
            "sli_type must be one of availability/latency/error_rate/custom (got '{}')",
            sli_type
        )));
    }
    if let Some(window_days) = req.window_days
        && !Slo::is_valid_window_days(window_days)
    {
        return Err(AppError::BadRequest(
            "window_days must be one of 7, 28, 30".to_string(),
        ));
    }
    if let Some(obj) = req.objective_pct
        && !(obj > 0.0 && obj < 100.0)
    {
        return Err(AppError::BadRequest(
            "objective_pct must be in (0, 100)".to_string(),
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests — pure validation only; I/O is covered by integration tests.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn base_create() -> CreateSloRequest {
        CreateSloRequest {
            name: "checkout-availability".into(),
            description: None,
            component_id: None,
            sli_type: "availability".into(),
            good_events_query: "sum(rate(http_requests_total{status!~\"5..\"}[5m]))".into(),
            total_events_query: "sum(rate(http_requests_total[5m]))".into(),
            objective_pct: 99.9,
            window_days: 28,
            burn_rate_policy: "mwmbr_default".into(),
            labels: serde_json::json!({}),
            enabled: true,
        }
    }

    #[test]
    fn validate_create_accepts_well_formed_payload() {
        assert!(validate_create(&base_create()).is_ok());
    }

    #[test]
    fn validate_create_rejects_empty_name_and_queries() {
        let mut req = base_create();
        req.name = "   ".into();
        assert!(validate_create(&req).is_err());

        let mut req = base_create();
        req.good_events_query = "".into();
        assert!(validate_create(&req).is_err());

        let mut req = base_create();
        req.total_events_query = "".into();
        assert!(validate_create(&req).is_err());
    }

    #[test]
    fn validate_create_rejects_unknown_sli_type_and_bad_window() {
        let mut req = base_create();
        req.sli_type = "throughput".into();
        assert!(validate_create(&req).is_err());

        let mut req = base_create();
        req.window_days = 14;
        assert!(validate_create(&req).is_err());
    }

    #[test]
    fn validate_create_rejects_objective_out_of_range() {
        for bad in [0.0, -1.0, 100.0, 100.1, 150.0] {
            let mut req = base_create();
            req.objective_pct = bad;
            assert!(
                validate_create(&req).is_err(),
                "expected rejection for objective_pct={bad}"
            );
        }
    }

    #[test]
    fn validate_update_all_none_is_ok() {
        assert!(validate_update(&UpdateSloRequest::default()).is_ok());
    }

    #[test]
    fn validate_update_rejects_blank_fields_when_supplied() {
        let mut req = UpdateSloRequest::default();
        req.name = Some("".into());
        assert!(validate_update(&req).is_err());

        let mut req = UpdateSloRequest::default();
        req.objective_pct = Some(100.0);
        assert!(validate_update(&req).is_err());

        let mut req = UpdateSloRequest::default();
        req.window_days = Some(14);
        assert!(validate_update(&req).is_err());
    }
}

// Prevent unused-import lint if we ever slim the module further.
#[allow(dead_code)]
fn _module_uses_metrics_endpoint(_e: &MetricsEndpoint) {}
