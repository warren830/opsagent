use axum::{
    Json,
    extract::{Path, Query, State},
};
use uuid::Uuid;

use crate::AppState;
use crate::error::{AppError, AppResult};
use crate::middleware::auth::AuthUser;
use crate::models::issue::{Issue, IssueListQuery, UpdateIssueRequest};

/// GET /api/issues
pub async fn list(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Query(query): Query<IssueListQuery>,
) -> AppResult<Json<Vec<Issue>>> {
    let rows = if auth_user.is_super_admin() {
        sqlx::query_as::<_, Issue>(
            r#"SELECT * FROM issues
               WHERE ($1::TEXT IS NULL OR status = $1)
                 AND ($2::TEXT IS NULL OR severity = $2)
               ORDER BY created_at DESC"#,
        )
        .bind(&query.status)
        .bind(&query.severity)
        .fetch_all(&state.pool)
        .await?
    } else {
        sqlx::query_as::<_, Issue>(
            r#"SELECT * FROM issues
               WHERE tenant_id = $1
                 AND ($2::TEXT IS NULL OR status = $2)
                 AND ($3::TEXT IS NULL OR severity = $3)
               ORDER BY created_at DESC"#,
        )
        .bind(auth_user.tenant_id)
        .bind(&query.status)
        .bind(&query.severity)
        .fetch_all(&state.pool)
        .await?
    };
    Ok(Json(rows))
}

/// GET /api/issues/:id
pub async fn get(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Issue>> {
    let row = sqlx::query_as::<_, Issue>("SELECT * FROM issues WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Issue not found".to_string()))?;

    if !auth_user.is_super_admin() && row.tenant_id != auth_user.tenant_id {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }

    Ok(Json(row))
}

/// PUT /api/issues/:id
pub async fn update(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateIssueRequest>,
) -> AppResult<Json<Issue>> {
    if !auth_user.is_super_admin() {
        let existing = sqlx::query_as::<_, Issue>("SELECT * FROM issues WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Issue not found".to_string()))?;
        if existing.tenant_id != auth_user.tenant_id {
            return Err(AppError::Forbidden("Access denied".to_string()));
        }
    }

    let row = sqlx::query_as::<_, Issue>(
        r#"UPDATE issues SET
           title = COALESCE($2, title),
           description = COALESCE($3, description),
           severity = COALESCE($4, severity),
           status = COALESCE($5, status),
           updated_at = NOW()
           WHERE id = $1
           RETURNING *"#,
    )
    .bind(id)
    .bind(&req.title)
    .bind(&req.description)
    .bind(&req.severity)
    .bind(&req.status)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Issue not found".to_string()))?;

    Ok(Json(row))
}

/// POST /api/issues/:id/rca (mock - sets rca_started_at)
pub async fn start_rca(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Issue>> {
    if !auth_user.is_super_admin() {
        let existing = sqlx::query_as::<_, Issue>("SELECT * FROM issues WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Issue not found".to_string()))?;
        if existing.tenant_id != auth_user.tenant_id {
            return Err(AppError::Forbidden("Access denied".to_string()));
        }
    }

    let row = sqlx::query_as::<_, Issue>(
        r#"UPDATE issues SET
           rca_started_at = NOW(),
           status = 'investigating',
           updated_at = NOW()
           WHERE id = $1
           RETURNING *"#,
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Issue not found".to_string()))?;

    Ok(Json(row))
}
