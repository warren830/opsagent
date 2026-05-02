//! Incident Command Center handlers — W2 CRUD, state transitions,
//! participants, and timeline queries.
//!
//! Routes (registered in `main.rs`):
//!
//! - `GET    /api/incidents` — list (filtered)
//! - `POST   /api/incidents` — create
//! - `GET    /api/incidents/active` — list non-closed incidents
//! - `GET    /api/incidents/:id` — detail (incident + last 20 timeline
//!                                 events + participants + last 5 updates)
//! - `PATCH  /api/incidents/:id` — edit metadata (NOT status/severity)
//! - `POST   /api/incidents/:id/transition` — status transition
//! - `POST   /api/incidents/:id/severity` — severity change (writes history)
//! - `POST   /api/incidents/:id/participants` — add participant
//! - `DELETE /api/incidents/:id/participants/:user_id/:role` — leave
//! - `GET    /api/incidents/:id/timeline` — paginated timeline query
//!
//! Tenant isolation mirrors `handlers/issue.rs`: super_admin sees every row,
//! everyone else is scoped to `tenant_id = auth_user.tenant_id`.

use axum::{
    Json,
    extract::{Path, Query, State},
};
use chrono::Utc;
use uuid::Uuid;

use crate::AppState;
use crate::error::{AppError, AppResult};
use crate::middleware::auth::AuthUser;
use crate::models::incident::{
    self, AddParticipantRequest, CreateIncidentRequest, Incident, IncidentDetail,
    IncidentParticipant, IncidentTimelineEvent, IncidentUpdate, ListIncidentsQuery,
    SeverityChangeRequest, TimelineQuery, TransitionRequest, UpdateIncidentRequest,
};
use crate::services::incident::{state_machine, timeline};

// ---------------------------------------------------------------------------
// Helpers — tenant-scoped fetch + auth guard.
// ---------------------------------------------------------------------------

/// Fetches a single incident and enforces tenant isolation. Returns
/// `NotFound` if the id is unknown or the user is not allowed to see it.
async fn fetch_and_check(
    state: &AppState,
    auth_user: &AuthUser,
    id: Uuid,
) -> AppResult<Incident> {
    let row = sqlx::query_as::<_, Incident>("SELECT * FROM incidents WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Incident not found".to_string()))?;

    if !auth_user.is_super_admin() && row.tenant_id != auth_user.tenant_id {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }
    Ok(row)
}

// ---------------------------------------------------------------------------
// GET /api/incidents
// ---------------------------------------------------------------------------
pub async fn list(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Query(q): Query<ListIncidentsQuery>,
) -> AppResult<Json<Vec<Incident>>> {
    // Validate filter values so the query doesn't wildcard silently.
    if let Some(s) = &q.status
        && !Incident::is_valid_status(s)
    {
        return Err(AppError::BadRequest(format!("invalid status: {s}")));
    }
    if let Some(s) = &q.severity
        && !Incident::is_valid_severity(s)
    {
        return Err(AppError::BadRequest(format!("invalid severity: {s}")));
    }

    let rows = if auth_user.is_super_admin() {
        sqlx::query_as::<_, Incident>(
            r#"SELECT * FROM incidents
               WHERE ($1::TEXT IS NULL OR status = $1)
                 AND ($2::TEXT IS NULL OR severity = $2)
                 AND ($3::UUID IS NULL OR $3 = ANY(affected_component_ids))
                 AND ($4::BOOL = FALSE OR status <> 'closed')
               ORDER BY detected_at DESC"#,
        )
        .bind(&q.status)
        .bind(&q.severity)
        .bind(q.component_id)
        .bind(q.active_only)
        .fetch_all(&state.pool)
        .await?
    } else {
        sqlx::query_as::<_, Incident>(
            r#"SELECT * FROM incidents
               WHERE tenant_id = $1
                 AND ($2::TEXT IS NULL OR status = $2)
                 AND ($3::TEXT IS NULL OR severity = $3)
                 AND ($4::UUID IS NULL OR $4 = ANY(affected_component_ids))
                 AND ($5::BOOL = FALSE OR status <> 'closed')
               ORDER BY detected_at DESC"#,
        )
        .bind(auth_user.tenant_id)
        .bind(&q.status)
        .bind(&q.severity)
        .bind(q.component_id)
        .bind(q.active_only)
        .fetch_all(&state.pool)
        .await?
    };
    Ok(Json(rows))
}

// ---------------------------------------------------------------------------
// GET /api/incidents/active
// ---------------------------------------------------------------------------
pub async fn list_active(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
) -> AppResult<Json<Vec<Incident>>> {
    let rows = if auth_user.is_super_admin() {
        sqlx::query_as::<_, Incident>(
            r#"SELECT * FROM incidents
               WHERE status <> 'closed'
               ORDER BY detected_at DESC"#,
        )
        .fetch_all(&state.pool)
        .await?
    } else {
        sqlx::query_as::<_, Incident>(
            r#"SELECT * FROM incidents
               WHERE tenant_id = $1 AND status <> 'closed'
               ORDER BY detected_at DESC"#,
        )
        .bind(auth_user.tenant_id)
        .fetch_all(&state.pool)
        .await?
    };
    Ok(Json(rows))
}

// ---------------------------------------------------------------------------
// GET /api/incidents/:id
// ---------------------------------------------------------------------------
pub async fn get(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<IncidentDetail>> {
    let inc = fetch_and_check(&state, &auth_user, id).await?;

    let timeline_rows = sqlx::query_as::<_, IncidentTimelineEvent>(
        r#"SELECT * FROM incident_timeline_events
           WHERE incident_id = $1
           ORDER BY occurred_at DESC
           LIMIT 20"#,
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await?;

    let participants = sqlx::query_as::<_, IncidentParticipant>(
        r#"SELECT * FROM incident_participants
           WHERE incident_id = $1
           ORDER BY joined_at ASC"#,
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await?;

    let recent_updates = sqlx::query_as::<_, IncidentUpdate>(
        r#"SELECT * FROM incident_updates
           WHERE incident_id = $1
           ORDER BY created_at DESC
           LIMIT 5"#,
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(IncidentDetail {
        incident: inc,
        timeline: timeline_rows,
        participants,
        recent_updates,
    }))
}

// ---------------------------------------------------------------------------
// POST /api/incidents
// ---------------------------------------------------------------------------
pub async fn create(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Json(req): Json<CreateIncidentRequest>,
) -> AppResult<Json<Incident>> {
    // --- validation -------------------------------------------------------
    if req.title.trim().is_empty() {
        return Err(AppError::BadRequest("title is required".to_string()));
    }
    if !Incident::is_valid_severity(&req.severity) {
        return Err(AppError::BadRequest(format!(
            "invalid severity: {}",
            req.severity
        )));
    }
    if !incident::ALL_DETECTION_SOURCES.contains(&req.detection_source.as_str()) {
        return Err(AppError::BadRequest(format!(
            "invalid detection_source: {}",
            req.detection_source
        )));
    }
    // `status` is always triggered on create — ignore any override.
    let status = incident::STATUS_TRIGGERED.to_string();

    // Tenant: super_admin may set any tenant_id via the request body, every-
    // one else is forced to their own tenant.
    let tenant_id = if auth_user.is_super_admin() {
        req.tenant_id.or(auth_user.tenant_id)
    } else {
        auth_user.tenant_id
    };

    let labels = req
        .labels
        .clone()
        .unwrap_or_else(|| serde_json::json!({}));

    let row = sqlx::query_as::<_, Incident>(
        r#"INSERT INTO incidents (
               tenant_id, title, severity, status, commander_user_id, scribe_user_id,
               impact_summary, affected_component_ids, affected_customer_tier,
               detection_source, source_issue_id, started_at, detected_at, bridge_url,
               labels
           )
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, NOW(), $13, $14)
           RETURNING *"#,
    )
    .bind(tenant_id)
    .bind(req.title.trim())
    .bind(&req.severity)
    .bind(&status)
    .bind(req.commander_user_id)
    .bind(req.scribe_user_id)
    .bind(&req.impact_summary)
    .bind(&req.affected_component_ids)
    .bind(&req.affected_customer_tier)
    .bind(&req.detection_source)
    .bind(req.source_issue_id)
    .bind(req.started_at)
    .bind(&req.bridge_url)
    .bind(&labels)
    .fetch_one(&state.pool)
    .await?;

    // Seed the timeline with a creation event so detail view has something
    // immediately. Failures here are logged but non-fatal — the incident
    // is still created.
    let actor = timeline::user_actor(auth_user.user_id, &auth_user.username);
    if let Err(e) = timeline::record_event(
        &state.pool,
        row.id,
        timeline::KIND_STATUS_CHANGED,
        actor,
        "Incident created",
        serde_json::json!({
            "from": null,
            "to": row.status,
            "detection_source": row.detection_source,
        }),
    )
    .await
    {
        tracing::warn!("timeline seed failed for incident {}: {}", row.id, e);
    }

    Ok(Json(row))
}

// ---------------------------------------------------------------------------
// PATCH /api/incidents/:id
// ---------------------------------------------------------------------------
pub async fn update(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateIncidentRequest>,
) -> AppResult<Json<Incident>> {
    // Auth guard.
    let _current = fetch_and_check(&state, &auth_user, id).await?;

    let row = sqlx::query_as::<_, Incident>(
        r#"UPDATE incidents SET
               title = COALESCE($2, title),
               impact_summary = COALESCE($3, impact_summary),
               affected_component_ids = COALESCE($4, affected_component_ids),
               affected_customer_tier = COALESCE($5, affected_customer_tier),
               commander_user_id = COALESCE($6, commander_user_id),
               scribe_user_id = COALESCE($7, scribe_user_id),
               labels = COALESCE($8, labels),
               root_cause = COALESCE($9, root_cause),
               root_cause_category = COALESCE($10, root_cause_category),
               updated_at = NOW()
           WHERE id = $1
           RETURNING *"#,
    )
    .bind(id)
    .bind(&req.title)
    .bind(&req.impact_summary)
    .bind(req.affected_component_ids.as_deref())
    .bind(&req.affected_customer_tier)
    .bind(req.commander_user_id)
    .bind(req.scribe_user_id)
    .bind(&req.labels)
    .bind(&req.root_cause)
    .bind(&req.root_cause_category)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Incident not found".to_string()))?;

    Ok(Json(row))
}

// ---------------------------------------------------------------------------
// POST /api/incidents/:id/transition
// ---------------------------------------------------------------------------
pub async fn transition(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<TransitionRequest>,
) -> AppResult<Json<Incident>> {
    let current = fetch_and_check(&state, &auth_user, id).await?;

    if !state_machine::can_transition(&current.status, &req.to_status) {
        return Err(AppError::BadRequest(format!(
            "invalid transition: {} -> {}",
            current.status, req.to_status
        )));
    }

    if state_machine::transition_requires_reason(&current.status, &req.to_status)
        && req
            .reason
            .as_ref()
            .map(|s| s.trim().is_empty())
            .unwrap_or(true)
    {
        return Err(AppError::BadRequest(
            "reason is required for regression transitions".to_string(),
        ));
    }

    // Decide which timestamp column to stamp based on the target status.
    let now = Utc::now();
    let (ack, mit, res, closed) = match req.to_status.as_str() {
        incident::STATUS_ACKNOWLEDGED => (Some(now), None, None, None),
        incident::STATUS_MITIGATED => (None, Some(now), None, None),
        incident::STATUS_RESOLVED => (None, None, Some(now), None),
        incident::STATUS_CLOSED => (None, None, None, Some(now)),
        _ => (None, None, None, None),
    };

    let row = sqlx::query_as::<_, Incident>(
        r#"UPDATE incidents SET
               status = $2,
               acknowledged_at = COALESCE(acknowledged_at, $3),
               mitigated_at    = COALESCE(mitigated_at,    $4),
               resolved_at     = COALESCE(resolved_at,     $5),
               closed_at       = COALESCE(closed_at,       $6),
               updated_at = NOW()
           WHERE id = $1
           RETURNING *"#,
    )
    .bind(id)
    .bind(&req.to_status)
    .bind(ack)
    .bind(mit)
    .bind(res)
    .bind(closed)
    .fetch_one(&state.pool)
    .await?;

    let actor = timeline::user_actor(auth_user.user_id, &auth_user.username);
    let summary = format!("Status: {} → {}", current.status, req.to_status);
    if let Err(e) = timeline::record_event(
        &state.pool,
        id,
        timeline::KIND_STATUS_CHANGED,
        actor,
        &summary,
        serde_json::json!({
            "from": current.status,
            "to": req.to_status,
            "reason": req.reason,
        }),
    )
    .await
    {
        tracing::warn!("timeline record failed for incident {}: {}", id, e);
    }

    Ok(Json(row))
}

// ---------------------------------------------------------------------------
// POST /api/incidents/:id/severity
// ---------------------------------------------------------------------------
pub async fn change_severity(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<SeverityChangeRequest>,
) -> AppResult<Json<Incident>> {
    let current = fetch_and_check(&state, &auth_user, id).await?;

    if !Incident::is_valid_severity(&req.to_severity) {
        return Err(AppError::BadRequest(format!(
            "invalid severity: {}",
            req.to_severity
        )));
    }
    if req.to_severity == current.severity {
        return Err(AppError::BadRequest(
            "new severity must differ from current".to_string(),
        ));
    }
    if req.reason.trim().is_empty() {
        return Err(AppError::BadRequest(
            "reason is required for severity change".to_string(),
        ));
    }

    let mut tx = state.pool.begin().await?;

    let row = sqlx::query_as::<_, Incident>(
        r#"UPDATE incidents SET severity = $2, updated_at = NOW()
           WHERE id = $1
           RETURNING *"#,
    )
    .bind(id)
    .bind(&req.to_severity)
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query(
        r#"INSERT INTO incident_severity_history
               (incident_id, from_severity, to_severity, changed_by, reason)
           VALUES ($1, $2, $3, $4, $5)"#,
    )
    .bind(id)
    .bind(&current.severity)
    .bind(&req.to_severity)
    .bind(auth_user.user_id)
    .bind(&req.reason)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    let actor = timeline::user_actor(auth_user.user_id, &auth_user.username);
    let summary = format!(
        "Severity: {} → {}",
        current.severity, req.to_severity
    );
    if let Err(e) = timeline::record_event(
        &state.pool,
        id,
        timeline::KIND_SEVERITY_CHANGED,
        actor,
        &summary,
        serde_json::json!({
            "from": current.severity,
            "to": req.to_severity,
            "reason": req.reason,
        }),
    )
    .await
    {
        tracing::warn!("timeline record failed for incident {}: {}", id, e);
    }

    Ok(Json(row))
}

// ---------------------------------------------------------------------------
// POST /api/incidents/:id/participants
// ---------------------------------------------------------------------------
pub async fn add_participant(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<AddParticipantRequest>,
) -> AppResult<Json<IncidentParticipant>> {
    let _ = fetch_and_check(&state, &auth_user, id).await?;

    if !incident::ALL_PARTICIPANT_ROLES.contains(&req.role.as_str()) {
        return Err(AppError::BadRequest(format!(
            "invalid role: {}",
            req.role
        )));
    }
    let added_via = req.added_via.clone().unwrap_or_else(|| "manual_invite".to_string());
    if !matches!(
        added_via.as_str(),
        "on_call_auto" | "manual_invite" | "self_join"
    ) {
        return Err(AppError::BadRequest(format!(
            "invalid added_via: {added_via}"
        )));
    }

    // ON CONFLICT DO NOTHING preserves the existing row (including joined_at).
    // We then fetch it so the caller always gets the current participant row.
    sqlx::query(
        r#"INSERT INTO incident_participants
               (incident_id, user_id, role, added_via)
           VALUES ($1, $2, $3, $4)
           ON CONFLICT (incident_id, user_id, role) DO NOTHING"#,
    )
    .bind(id)
    .bind(req.user_id)
    .bind(&req.role)
    .bind(&added_via)
    .execute(&state.pool)
    .await?;

    let part = sqlx::query_as::<_, IncidentParticipant>(
        r#"SELECT * FROM incident_participants
           WHERE incident_id = $1 AND user_id = $2 AND role = $3"#,
    )
    .bind(id)
    .bind(req.user_id)
    .bind(&req.role)
    .fetch_one(&state.pool)
    .await?;

    let actor = timeline::user_actor(auth_user.user_id, &auth_user.username);
    let summary = format!("Joined as {}", req.role);
    if let Err(e) = timeline::record_event(
        &state.pool,
        id,
        timeline::KIND_PARTICIPANT_JOIN,
        actor,
        &summary,
        serde_json::json!({
            "user_id": req.user_id,
            "role": req.role,
            "added_via": added_via,
        }),
    )
    .await
    {
        tracing::warn!("timeline record failed for incident {}: {}", id, e);
    }

    Ok(Json(part))
}

// ---------------------------------------------------------------------------
// DELETE /api/incidents/:id/participants/:user_id/:role
// ---------------------------------------------------------------------------
pub async fn remove_participant(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path((id, user_id, role)): Path<(Uuid, Uuid, String)>,
) -> AppResult<Json<serde_json::Value>> {
    let _ = fetch_and_check(&state, &auth_user, id).await?;

    let rows = sqlx::query(
        r#"UPDATE incident_participants
           SET left_at = NOW()
           WHERE incident_id = $1 AND user_id = $2 AND role = $3
             AND left_at IS NULL"#,
    )
    .bind(id)
    .bind(user_id)
    .bind(&role)
    .execute(&state.pool)
    .await?
    .rows_affected();

    if rows == 0 {
        return Err(AppError::NotFound(
            "participant not found or already left".to_string(),
        ));
    }

    let actor = timeline::user_actor(auth_user.user_id, &auth_user.username);
    let summary = format!("Left as {role}");
    if let Err(e) = timeline::record_event(
        &state.pool,
        id,
        timeline::KIND_PARTICIPANT_LEAVE,
        actor,
        &summary,
        serde_json::json!({
            "user_id": user_id,
            "role": role,
        }),
    )
    .await
    {
        tracing::warn!("timeline record failed for incident {}: {}", id, e);
    }

    Ok(Json(serde_json::json!({ "removed": true })))
}

// ---------------------------------------------------------------------------
// GET /api/incidents/:id/timeline
// ---------------------------------------------------------------------------
pub async fn list_timeline(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(q): Query<TimelineQuery>,
) -> AppResult<Json<Vec<IncidentTimelineEvent>>> {
    let _ = fetch_and_check(&state, &auth_user, id).await?;

    // Clamp the limit to avoid runaway queries. Default 100, max 500.
    let limit = q.limit.unwrap_or(100).clamp(1, 500);

    let rows = sqlx::query_as::<_, IncidentTimelineEvent>(
        r#"SELECT * FROM incident_timeline_events
           WHERE incident_id = $1
             AND ($2::TIMESTAMPTZ IS NULL OR occurred_at > $2)
             AND ($3::TEXT IS NULL OR kind = $3)
           ORDER BY occurred_at DESC
           LIMIT $4"#,
    )
    .bind(id)
    .bind(q.after)
    .bind(&q.kind)
    .bind(limit)
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(rows))
}
