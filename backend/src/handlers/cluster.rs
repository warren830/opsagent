use axum::{
    Json,
    extract::{Path, State},
};
use uuid::Uuid;

use crate::AppState;
use crate::error::{AppError, AppResult};
use crate::middleware::auth::AuthUser;
use crate::models::cluster::{Cluster, CreateClusterRequest, UpdateClusterRequest};

/// GET /api/clusters
pub async fn list(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
) -> AppResult<Json<Vec<Cluster>>> {
    let rows = if auth_user.is_super_admin() {
        sqlx::query_as::<_, Cluster>("SELECT * FROM clusters ORDER BY name")
            .fetch_all(&state.pool)
            .await?
    } else {
        sqlx::query_as::<_, Cluster>("SELECT * FROM clusters WHERE tenant_id = $1 ORDER BY name")
            .bind(auth_user.tenant_id)
            .fetch_all(&state.pool)
            .await?
    };
    Ok(Json(rows))
}

/// POST /api/clusters
pub async fn create(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Json(req): Json<CreateClusterRequest>,
) -> AppResult<Json<Cluster>> {
    if req.name.trim().is_empty() {
        return Err(AppError::BadRequest("Name is required".to_string()));
    }

    let row = sqlx::query_as::<_, Cluster>(
        r#"INSERT INTO clusters (name, cloud, cluster_type, account_id, region, role_name, description, config, tenant_id)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
           RETURNING *"#,
    )
    .bind(&req.name)
    .bind(&req.cloud)
    .bind(&req.cluster_type)
    .bind(&req.account_id)
    .bind(&req.region)
    .bind(&req.role_name)
    .bind(&req.description)
    .bind(&req.config)
    .bind(auth_user.tenant_id)
    .fetch_one(&state.pool)
    .await?;

    Ok(Json(row))
}

/// PUT /api/clusters/:id
pub async fn update(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateClusterRequest>,
) -> AppResult<Json<Cluster>> {
    if !auth_user.is_super_admin() {
        let existing = sqlx::query_as::<_, Cluster>("SELECT * FROM clusters WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Cluster not found".to_string()))?;
        if existing.tenant_id != auth_user.tenant_id {
            return Err(AppError::Forbidden("Access denied".to_string()));
        }
    }

    let row = sqlx::query_as::<_, Cluster>(
        r#"UPDATE clusters SET
           name = COALESCE($2, name),
           cloud = COALESCE($3, cloud),
           cluster_type = COALESCE($4, cluster_type),
           account_id = COALESCE($5, account_id),
           region = COALESCE($6, region),
           role_name = COALESCE($7, role_name),
           description = COALESCE($8, description),
           status = COALESCE($9, status),
           config = COALESCE($10, config),
           updated_at = NOW()
           WHERE id = $1
           RETURNING *"#,
    )
    .bind(id)
    .bind(&req.name)
    .bind(&req.cloud)
    .bind(&req.cluster_type)
    .bind(&req.account_id)
    .bind(&req.region)
    .bind(&req.role_name)
    .bind(&req.description)
    .bind(&req.status)
    .bind(&req.config)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Cluster not found".to_string()))?;

    Ok(Json(row))
}

/// DELETE /api/clusters/:id
pub async fn delete(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    if !auth_user.is_super_admin() {
        let existing = sqlx::query_as::<_, Cluster>("SELECT * FROM clusters WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Cluster not found".to_string()))?;
        if existing.tenant_id != auth_user.tenant_id {
            return Err(AppError::Forbidden("Access denied".to_string()));
        }
    }

    let result = sqlx::query("DELETE FROM clusters WHERE id = $1")
        .bind(id)
        .execute(&state.pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Cluster not found".to_string()));
    }

    Ok(Json(serde_json::json!({"message": "Cluster deleted"})))
}
