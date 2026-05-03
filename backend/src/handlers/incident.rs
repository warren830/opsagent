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
    response::sse::{Event, Sse},
};
use chrono::Utc;
use std::convert::Infallible;
use std::pin::Pin;
use std::time::Duration;
use tokio_stream::{Stream, StreamExt, wrappers::BroadcastStream};
use uuid::Uuid;

use crate::AppState;
use crate::error::{AppError, AppResult};
use crate::middleware::auth::AuthUser;
use crate::models::incident::{
    self, AddParticipantRequest, CreateIncidentRequest, CreateTimelineNoteRequest,
    CreateUpdateRequest, Incident, IncidentDetail, IncidentParticipant, IncidentTimelineEvent,
    IncidentUpdate, ListIncidentsQuery, PostmortemDoc, SeverityChangeRequest, TimelineQuery,
    TransitionRequest, UpdateIncidentRequest, UpdatePostmortemRequest,
};
use crate::services::incident::{postmortem_drafter, state_machine, timeline};

type SseEventStream = Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>>;

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

/// Confirm that `user_id` exists and either belongs to `tenant_id` or has
/// NULL tenant_id (tenant-agnostic). Used before accepting user-referencing
/// fields on incident create/update so callers can't assign cross-tenant
/// users as commander/scribe.
async fn validate_user_in_tenant(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    tenant_id: Option<Uuid>,
) -> AppResult<()> {
    let ok: bool = sqlx::query_scalar(
        r#"SELECT EXISTS (
               SELECT 1 FROM users
               WHERE id = $1
                 AND (tenant_id = $2 OR tenant_id IS NULL)
           )"#,
    )
    .bind(user_id)
    .bind(tenant_id)
    .fetch_one(pool)
    .await?;
    if !ok {
        return Err(AppError::BadRequest(format!(
            "user {user_id} is not a member of the incident's tenant"
        )));
    }
    Ok(())
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

    // If the caller named an incident commander or scribe, confirm the
    // referenced user belongs to the same tenant (or is tenant-agnostic)
    // before accepting the row. Otherwise a caller could assign cross-tenant
    // users as commander/scribe and silently leak membership across tenants.
    if let Some(uid) = req.commander_user_id {
        validate_user_in_tenant(&state.pool, uid, tenant_id).await?;
    }
    if let Some(uid) = req.scribe_user_id {
        validate_user_in_tenant(&state.pool, uid, tenant_id).await?;
    }

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
        &state.timeline_bus,
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
    let current = fetch_and_check(&state, &auth_user, id).await?;

    // Cross-tenant user assignment guard — mirror of the create() path.
    if let Some(uid) = req.commander_user_id {
        validate_user_in_tenant(&state.pool, uid, current.tenant_id).await?;
    }
    if let Some(uid) = req.scribe_user_id {
        validate_user_in_tenant(&state.pool, uid, current.tenant_id).await?;
    }

    // The UPDATE itself is tenant-scoped for non–super-admin callers so a
    // race between fetch_and_check and the write cannot mutate another
    // tenant's row.
    let row_opt = if auth_user.is_super_admin() {
        sqlx::query_as::<_, Incident>(
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
    } else {
        let tenant_id = auth_user
            .tenant_id
            .ok_or_else(|| AppError::Forbidden("No tenant context".to_string()))?;
        sqlx::query_as::<_, Incident>(
            r#"UPDATE incidents SET
                   title = COALESCE($3, title),
                   impact_summary = COALESCE($4, impact_summary),
                   affected_component_ids = COALESCE($5, affected_component_ids),
                   affected_customer_tier = COALESCE($6, affected_customer_tier),
                   commander_user_id = COALESCE($7, commander_user_id),
                   scribe_user_id = COALESCE($8, scribe_user_id),
                   labels = COALESCE($9, labels),
                   root_cause = COALESCE($10, root_cause),
                   root_cause_category = COALESCE($11, root_cause_category),
                   updated_at = NOW()
               WHERE id = $1 AND tenant_id = $2
               RETURNING *"#,
        )
        .bind(id)
        .bind(tenant_id)
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
    };
    let row = row_opt.ok_or_else(|| AppError::NotFound("Incident not found".to_string()))?;

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

    let row_opt = if auth_user.is_super_admin() {
        sqlx::query_as::<_, Incident>(
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
        .fetch_optional(&state.pool)
        .await?
    } else {
        let tenant_id = auth_user
            .tenant_id
            .ok_or_else(|| AppError::Forbidden("No tenant context".to_string()))?;
        sqlx::query_as::<_, Incident>(
            r#"UPDATE incidents SET
                   status = $3,
                   acknowledged_at = COALESCE(acknowledged_at, $4),
                   mitigated_at    = COALESCE(mitigated_at,    $5),
                   resolved_at     = COALESCE(resolved_at,     $6),
                   closed_at       = COALESCE(closed_at,       $7),
                   updated_at = NOW()
               WHERE id = $1 AND tenant_id = $2
               RETURNING *"#,
        )
        .bind(id)
        .bind(tenant_id)
        .bind(&req.to_status)
        .bind(ack)
        .bind(mit)
        .bind(res)
        .bind(closed)
        .fetch_optional(&state.pool)
        .await?
    };
    let row = row_opt.ok_or_else(|| AppError::NotFound("Incident not found".to_string()))?;

    let actor = timeline::user_actor(auth_user.user_id, &auth_user.username);
    let summary = format!("Status: {} → {}", current.status, req.to_status);
    if let Err(e) = timeline::record_event(
        &state.pool,
        &state.timeline_bus,
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

    // Auto-draft postmortem when a sev1/sev2 incident enters `resolved`.
    // Fire-and-forget — failures are logged but do not bubble up into
    // the transition response.
    //
    // NOTE: we do NOT auto-create Jira tickets for action items here
    // because the default draft ships a placeholder-only Action Items
    // table (`postmortem_drafter::parse_action_items` returns empty).
    // The IC fills the table in `/incidents/:id/postmortem` and the
    // editor page calls `POST /api/jira/create` per row.
    if req.to_status == incident::STATUS_RESOLVED
        && matches!(
            current.severity.as_str(),
            incident::SEVERITY_SEV1 | incident::SEVERITY_SEV2
        )
    {
        let pool_bg = state.pool.clone();
        let bus_bg = state.timeline_bus.clone();
        let tenant_id = row.tenant_id;
        let number = row.number;
        let created_by = auth_user.user_id;
        let incident_id_bg = id;
        tokio::spawn(async move {
            match crate::services::incident::postmortem_drafter::draft(&pool_bg, incident_id_bg)
                .await
            {
                Ok(draft) => {
                    let filename = format!("postmortem-INC-{number:04}.md");
                    let kf_result: Result<(Uuid,), sqlx::Error> = sqlx::query_as(
                        r#"INSERT INTO knowledge_files
                               (filename, content, size_bytes, mime_type, tenant_id, created_by, source)
                           VALUES ($1, $2, $3, 'text/markdown', $4, $5, 'postmortem')
                           RETURNING id"#,
                    )
                    .bind(&filename)
                    .bind(&draft.markdown)
                    .bind(draft.markdown.len() as i64)
                    .bind(tenant_id)
                    .bind(created_by)
                    .fetch_one(&pool_bg)
                    .await;

                    let kf_id = match kf_result {
                        Ok((kid,)) => kid,
                        Err(e) => {
                            tracing::warn!(
                                "postmortem auto-draft: insert knowledge_file failed: {e}"
                            );
                            return;
                        }
                    };

                    if let Err(e) = sqlx::query(
                        r#"UPDATE incidents
                           SET postmortem_doc_ref = jsonb_build_object(
                                   'knowledge_file_id', $2::text,
                                   'filename', $3::text
                               ),
                               updated_at = NOW()
                           WHERE id = $1"#,
                    )
                    .bind(incident_id_bg)
                    .bind(kf_id.to_string())
                    .bind(&filename)
                    .execute(&pool_bg)
                    .await
                    {
                        tracing::warn!("postmortem auto-draft: incident update failed: {e}");
                    }

                    let actor = crate::services::incident::timeline::system_actor(
                        "postmortem_drafter",
                    );
                    if let Err(e) = crate::services::incident::timeline::record_event(
                        &pool_bg,
                        &bus_bg,
                        incident_id_bg,
                        "postmortem_draft_ready",
                        actor,
                        "Postmortem draft generated automatically",
                        serde_json::json!({
                            "knowledge_file_id": kf_id,
                            "auto": true,
                        }),
                    )
                    .await
                    {
                        tracing::warn!("postmortem auto-draft: timeline record failed: {e}");
                    }
                }
                Err(e) => {
                    let actor = crate::services::incident::timeline::system_actor(
                        "postmortem_drafter",
                    );
                    let _ = crate::services::incident::timeline::record_event(
                        &pool_bg,
                        &bus_bg,
                        incident_id_bg,
                        "postmortem_draft_error",
                        actor,
                        "Postmortem auto-draft failed",
                        serde_json::json!({ "error": e.to_string() }),
                    )
                    .await;
                    tracing::warn!("postmortem auto-draft failed: {e}");
                }
            }
        });
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

    // Only the incident commander, a tenant admin, or a super admin may
    // change severity. Arbitrary tenant members can still see and update
    // metadata but must not be able to silently downgrade an ongoing Sev1
    // (or upgrade to one) — that decision belongs to the IC chain.
    let is_authorized = auth_user.is_super_admin()
        || auth_user.is_tenant_admin()
        || current.commander_user_id == Some(auth_user.user_id);
    if !is_authorized {
        return Err(AppError::Forbidden(
            "Only the incident commander or a tenant admin can change severity".to_string(),
        ));
    }

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

    let row_opt = if auth_user.is_super_admin() {
        sqlx::query_as::<_, Incident>(
            r#"UPDATE incidents SET severity = $2, updated_at = NOW()
               WHERE id = $1
               RETURNING *"#,
        )
        .bind(id)
        .bind(&req.to_severity)
        .fetch_optional(&mut *tx)
        .await?
    } else {
        let tenant_id = auth_user
            .tenant_id
            .ok_or_else(|| AppError::Forbidden("No tenant context".to_string()))?;
        sqlx::query_as::<_, Incident>(
            r#"UPDATE incidents SET severity = $3, updated_at = NOW()
               WHERE id = $1 AND tenant_id = $2
               RETURNING *"#,
        )
        .bind(id)
        .bind(tenant_id)
        .bind(&req.to_severity)
        .fetch_optional(&mut *tx)
        .await?
    };
    let row = row_opt.ok_or_else(|| AppError::NotFound("Incident not found".to_string()))?;

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
        &state.timeline_bus,
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
        &state.timeline_bus,
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
        &state.timeline_bus,
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

// ---------------------------------------------------------------------------
// GET /api/incidents/:id/timeline/stream (W4)
// ---------------------------------------------------------------------------
/// SSE endpoint that first flushes the most recent 50 timeline events
/// (oldest-first) and then keeps the connection open, pushing new events
/// live as writers publish onto `TimelineBus`.
///
/// SSE event name: `timeline.event`. Each `data` frame is the full
/// `IncidentTimelineEvent` JSON.
///
/// If the broadcast channel lags (a slow subscriber), a synthetic
/// `stream.lagged` event is emitted so the client knows to reload from
/// the paginated endpoint. The stream keeps running.
pub async fn stream_timeline(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Sse<axum::response::sse::KeepAliveStream<SseEventStream>>> {
    // Auth + tenant check. We intentionally load the full row to fail
    // early if the incident was deleted.
    let _inc = fetch_and_check(&state, &auth_user, id).await?;

    // Subscribe BEFORE reading the backlog. Otherwise a race: a publish
    // that lands between the SELECT and subscribe would be lost.
    let rx = state.timeline_bus.subscribe();

    // Backlog — 50 most recent events, in chronological order so the
    // client can append them as they arrive.
    let backlog = sqlx::query_as::<_, IncidentTimelineEvent>(
        r#"SELECT * FROM incident_timeline_events
           WHERE incident_id = $1
           ORDER BY occurred_at DESC
           LIMIT 50"#,
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await?;
    // Reverse into chronological order for easy client-side append.
    let mut backlog = backlog;
    backlog.reverse();
    let backlog_ids: std::collections::HashSet<Uuid> =
        backlog.iter().map(|e| e.id).collect();

    let incident_id = id;

    let stream = async_stream::stream! {
        // 1. Flush backlog.
        for ev in backlog {
            let data = serde_json::to_string(&ev).unwrap_or_default();
            yield Ok::<_, Infallible>(
                Event::default().event("timeline.event").data(data),
            );
        }

        // 2. Subscribe + filter loop.
        let mut bs = BroadcastStream::new(rx);
        while let Some(result) = bs.next().await {
            match result {
                Ok(broadcast) => {
                    if broadcast.incident_id != incident_id {
                        continue;
                    }
                    // Dedup: skip any event whose id we already flushed.
                    if backlog_ids.contains(&broadcast.event.id) {
                        continue;
                    }
                    let data = serde_json::to_string(&broadcast.event).unwrap_or_default();
                    yield Ok::<_, Infallible>(
                        Event::default().event("timeline.event").data(data),
                    );
                }
                Err(_lag) => {
                    // Subscriber fell behind. Notify the client and keep going.
                    yield Ok::<_, Infallible>(
                        Event::default()
                            .event("stream.lagged")
                            .data(r#"{"hint":"reload_backlog"}"#),
                    );
                }
            }
        }
    };

    let boxed: SseEventStream = Box::pin(stream);
    Ok(Sse::new(boxed).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("ping"),
    ))
}

// ---------------------------------------------------------------------------
// POST /api/incidents/:id/timeline  (W5 — manual note from the war room UI)
// ---------------------------------------------------------------------------
/// Drop a free-form note onto the timeline. Used by the "Add note" input
/// in the war-room page. The note always carries `kind=manual_note` unless
/// the caller overrides it with one of the registered kinds.
pub async fn create_timeline_note(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<CreateTimelineNoteRequest>,
) -> AppResult<Json<IncidentTimelineEvent>> {
    let _inc = fetch_and_check(&state, &auth_user, id).await?;

    if req.summary.trim().is_empty() {
        return Err(AppError::BadRequest("summary is required".to_string()));
    }
    let kind = req.kind.clone().unwrap_or_else(|| "manual_note".to_string());
    let payload = req
        .payload
        .clone()
        .unwrap_or_else(|| serde_json::json!({}));

    let actor = timeline::user_actor(auth_user.user_id, &auth_user.username);
    let row = sqlx::query_as::<_, IncidentTimelineEvent>(
        r#"INSERT INTO incident_timeline_events
               (incident_id, kind, actor, summary, payload)
           VALUES ($1, $2, $3, $4, $5)
           RETURNING *"#,
    )
    .bind(id)
    .bind(&kind)
    .bind(&actor)
    .bind(req.summary.trim())
    .bind(&payload)
    .fetch_one(&state.pool)
    .await?;

    // Fan out to SSE subscribers so the war room refreshes instantly.
    state
        .timeline_bus
        .publish(crate::services::incident::timeline_bus::TimelineBroadcast {
            incident_id: id,
            event: row.clone(),
        });

    Ok(Json(row))
}

// ---------------------------------------------------------------------------
// POST /api/incidents/:id/updates  (W5 — stakeholder communication)
// ---------------------------------------------------------------------------
pub async fn create_update(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<CreateUpdateRequest>,
) -> AppResult<Json<IncidentUpdate>> {
    let inc = fetch_and_check(&state, &auth_user, id).await?;

    if !incident::ALL_UPDATE_AUDIENCES.contains(&req.audience.as_str()) {
        return Err(AppError::BadRequest(format!(
            "invalid audience: {}",
            req.audience
        )));
    }
    if req.body_markdown.trim().is_empty() {
        return Err(AppError::BadRequest(
            "body_markdown is required".to_string(),
        ));
    }

    let published_at = if req.publish { Some(Utc::now()) } else { None };

    let row = sqlx::query_as::<_, IncidentUpdate>(
        r#"INSERT INTO incident_updates
               (incident_id, author_user_id, audience, status_at_time,
                body_markdown, published_at, pushed_to)
           VALUES ($1, $2, $3, $4, $5, $6, '{}'::jsonb)
           RETURNING *"#,
    )
    .bind(id)
    .bind(auth_user.user_id)
    .bind(&req.audience)
    .bind(&inc.status)
    .bind(req.body_markdown.trim())
    .bind(published_at)
    .fetch_one(&state.pool)
    .await?;

    if req.publish {
        // Broadcast a timeline event so subscribers see the update land.
        let actor = timeline::user_actor(auth_user.user_id, &auth_user.username);
        let summary = format!(
            "Update published to `{}` ({} chars)",
            req.audience,
            req.body_markdown.chars().count()
        );
        if let Err(e) = timeline::record_event(
            &state.pool,
            &state.timeline_bus,
            id,
            timeline::KIND_UPDATE_PUBLISHED,
            actor,
            &summary,
            serde_json::json!({
                "update_id": row.id,
                "audience": req.audience,
            }),
        )
        .await
        {
            tracing::warn!("timeline record failed for update: {}", e);
        }
    }

    Ok(Json(row))
}

// ---------------------------------------------------------------------------
// GET /api/incidents/:id/updates
// ---------------------------------------------------------------------------
pub async fn list_updates(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Vec<IncidentUpdate>>> {
    let _inc = fetch_and_check(&state, &auth_user, id).await?;
    let rows = sqlx::query_as::<_, IncidentUpdate>(
        r#"SELECT * FROM incident_updates
           WHERE incident_id = $1
           ORDER BY created_at DESC"#,
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(rows))
}

// ---------------------------------------------------------------------------
// GET /api/incidents/:id/postmortem  (W6)
// ---------------------------------------------------------------------------
/// Returns the postmortem markdown if one exists for this incident.
/// Resolution:
/// 1. `incidents.postmortem_doc_ref.knowledge_file_id` → `knowledge_files.content`
/// 2. if absent → returns `{ status: "absent", markdown: null }`
/// 3. if the ref exists but the row is gone → returns status `"missing"`
///    so the client can prompt for a fresh draft.
pub async fn get_postmortem(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<PostmortemDoc>> {
    let inc = fetch_and_check(&state, &auth_user, id).await?;
    let kf_id = inc
        .postmortem_doc_ref
        .as_ref()
        .and_then(|v| v.get("knowledge_file_id"))
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok());

    let Some(kf_id) = kf_id else {
        return Ok(Json(PostmortemDoc {
            incident_id: id,
            knowledge_file_id: None,
            status: "absent".to_string(),
            markdown: None,
            updated_at: None,
        }));
    };

    let row: Option<(String, chrono::DateTime<Utc>)> = sqlx::query_as(
        "SELECT content, updated_at FROM knowledge_files WHERE id = $1",
    )
    .bind(kf_id)
    .fetch_optional(&state.pool)
    .await?;

    match row {
        Some((content, updated_at)) => Ok(Json(PostmortemDoc {
            incident_id: id,
            knowledge_file_id: Some(kf_id),
            status: if matches!(inc.status.as_str(), incident::STATUS_POSTMORTEM_PUBLISHED) {
                "published".to_string()
            } else {
                "draft".to_string()
            },
            markdown: Some(content),
            updated_at: Some(updated_at),
        })),
        None => Ok(Json(PostmortemDoc {
            incident_id: id,
            knowledge_file_id: Some(kf_id),
            status: "missing".to_string(),
            markdown: None,
            updated_at: None,
        })),
    }
}

// ---------------------------------------------------------------------------
// PATCH /api/incidents/:id/postmortem  (W6 — IC edits the draft)
// ---------------------------------------------------------------------------
pub async fn update_postmortem(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdatePostmortemRequest>,
) -> AppResult<Json<PostmortemDoc>> {
    let inc = fetch_and_check(&state, &auth_user, id).await?;
    if req.markdown.trim().is_empty() {
        return Err(AppError::BadRequest("markdown is required".to_string()));
    }

    let kf_id = inc
        .postmortem_doc_ref
        .as_ref()
        .and_then(|v| v.get("knowledge_file_id"))
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok());

    let (kf_id, updated_at): (Uuid, chrono::DateTime<Utc>) = match kf_id {
        Some(existing) => {
            let row: (Uuid, chrono::DateTime<Utc>) = sqlx::query_as(
                r#"UPDATE knowledge_files SET
                       content = $2,
                       size_bytes = $3,
                       updated_at = NOW()
                   WHERE id = $1
                   RETURNING id, updated_at"#,
            )
            .bind(existing)
            .bind(&req.markdown)
            .bind(req.markdown.len() as i64)
            .fetch_one(&state.pool)
            .await?;
            row
        }
        None => {
            // Draft never materialized — create the knowledge row now.
            let filename = format!("postmortem-INC-{:04}.md", inc.number);
            let row: (Uuid, chrono::DateTime<Utc>) = sqlx::query_as(
                r#"INSERT INTO knowledge_files
                       (filename, content, size_bytes, mime_type, tenant_id, created_by, source)
                   VALUES ($1, $2, $3, 'text/markdown', $4, $5, 'postmortem')
                   RETURNING id, updated_at"#,
            )
            .bind(&filename)
            .bind(&req.markdown)
            .bind(req.markdown.len() as i64)
            .bind(inc.tenant_id)
            .bind(auth_user.user_id)
            .fetch_one(&state.pool)
            .await?;

            sqlx::query(
                r#"UPDATE incidents
                   SET postmortem_doc_ref = jsonb_build_object(
                           'knowledge_file_id', $2::text,
                           'filename', $3::text
                       ),
                       updated_at = NOW()
                   WHERE id = $1"#,
            )
            .bind(id)
            .bind(row.0.to_string())
            .bind(&filename)
            .execute(&state.pool)
            .await?;
            row
        }
    };

    Ok(Json(PostmortemDoc {
        incident_id: id,
        knowledge_file_id: Some(kf_id),
        status: if matches!(inc.status.as_str(), incident::STATUS_POSTMORTEM_PUBLISHED) {
            "published".to_string()
        } else {
            "draft".to_string()
        },
        markdown: Some(req.markdown),
        updated_at: Some(updated_at),
    }))
}

// ---------------------------------------------------------------------------
// POST /api/incidents/:id/postmortem/draft  (W6)
// ---------------------------------------------------------------------------
/// Kick the drafter. Runs synchronously — the template render is cheap
/// (<50ms typical) so we block and return the markdown immediately. The
/// resulting knowledge_file row is linked via `incidents.postmortem_doc_ref`
/// and a timeline event is written so observers see the draft land.
pub async fn draft_postmortem(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<PostmortemDoc>> {
    let inc = fetch_and_check(&state, &auth_user, id).await?;

    let draft = postmortem_drafter::draft(&state.pool, id).await?;

    // Upsert knowledge_files row.
    let kf_id = inc
        .postmortem_doc_ref
        .as_ref()
        .and_then(|v| v.get("knowledge_file_id"))
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok());

    let filename = format!("postmortem-INC-{:04}.md", inc.number);
    let (kf_id, updated_at): (Uuid, chrono::DateTime<Utc>) = match kf_id {
        Some(existing) => {
            sqlx::query_as(
                r#"UPDATE knowledge_files SET
                       content = $2,
                       size_bytes = $3,
                       updated_at = NOW()
                   WHERE id = $1
                   RETURNING id, updated_at"#,
            )
            .bind(existing)
            .bind(&draft.markdown)
            .bind(draft.markdown.len() as i64)
            .fetch_one(&state.pool)
            .await?
        }
        None => {
            let row: (Uuid, chrono::DateTime<Utc>) = sqlx::query_as(
                r#"INSERT INTO knowledge_files
                       (filename, content, size_bytes, mime_type, tenant_id, created_by, source)
                   VALUES ($1, $2, $3, 'text/markdown', $4, $5, 'postmortem')
                   RETURNING id, updated_at"#,
            )
            .bind(&filename)
            .bind(&draft.markdown)
            .bind(draft.markdown.len() as i64)
            .bind(inc.tenant_id)
            .bind(auth_user.user_id)
            .fetch_one(&state.pool)
            .await?;

            sqlx::query(
                r#"UPDATE incidents
                   SET postmortem_doc_ref = jsonb_build_object(
                           'knowledge_file_id', $2::text,
                           'filename', $3::text
                       ),
                       updated_at = NOW()
                   WHERE id = $1"#,
            )
            .bind(id)
            .bind(row.0.to_string())
            .bind(&filename)
            .execute(&state.pool)
            .await?;
            row
        }
    };

    let actor = timeline::user_actor(auth_user.user_id, &auth_user.username);
    if let Err(e) = timeline::record_event(
        &state.pool,
        &state.timeline_bus,
        id,
        "postmortem_draft_ready",
        actor,
        "Postmortem draft ready for review",
        serde_json::json!({ "knowledge_file_id": kf_id }),
    )
    .await
    {
        tracing::warn!("timeline record failed for postmortem draft: {}", e);
    }

    Ok(Json(PostmortemDoc {
        incident_id: id,
        knowledge_file_id: Some(kf_id),
        status: "draft".to_string(),
        markdown: Some(draft.markdown),
        updated_at: Some(updated_at),
    }))
}

// ---------------------------------------------------------------------------
// POST /api/incidents/:id/postmortem/publish  (W6)
// ---------------------------------------------------------------------------
/// Transition `status` → `postmortem_published` and flip the knowledge
/// file source marker so it shows up in the searchable knowledge base.
pub async fn publish_postmortem(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<PostmortemDoc>> {
    let inc = fetch_and_check(&state, &auth_user, id).await?;

    let target = incident::STATUS_POSTMORTEM_PUBLISHED;
    if !state_machine::can_transition(&inc.status, target) {
        return Err(AppError::BadRequest(format!(
            "cannot publish from status `{}`",
            inc.status
        )));
    }

    let kf_id = inc
        .postmortem_doc_ref
        .as_ref()
        .and_then(|v| v.get("knowledge_file_id"))
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| {
            AppError::BadRequest("postmortem not drafted yet".to_string())
        })?;

    let mut tx = state.pool.begin().await?;

    sqlx::query("UPDATE incidents SET status = $2, updated_at = NOW() WHERE id = $1")
        .bind(id)
        .bind(target)
        .execute(&mut *tx)
        .await?;

    let (_, updated_at): (Uuid, chrono::DateTime<Utc>) = sqlx::query_as(
        r#"UPDATE knowledge_files SET
               source = 'postmortem_published',
               updated_at = NOW()
           WHERE id = $1
           RETURNING id, updated_at"#,
    )
    .bind(kf_id)
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;

    let actor = timeline::user_actor(auth_user.user_id, &auth_user.username);
    if let Err(e) = timeline::record_event(
        &state.pool,
        &state.timeline_bus,
        id,
        timeline::KIND_STATUS_CHANGED,
        actor,
        "Postmortem published",
        serde_json::json!({
            "from": inc.status,
            "to": target,
            "knowledge_file_id": kf_id,
        }),
    )
    .await
    {
        tracing::warn!("timeline record failed for publish: {}", e);
    }

    let content: String = sqlx::query_scalar("SELECT content FROM knowledge_files WHERE id = $1")
        .bind(kf_id)
        .fetch_one(&state.pool)
        .await?;

    Ok(Json(PostmortemDoc {
        incident_id: id,
        knowledge_file_id: Some(kf_id),
        status: "published".to_string(),
        markdown: Some(content),
        updated_at: Some(updated_at),
    }))
}
