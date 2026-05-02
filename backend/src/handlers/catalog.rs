//! Catalog HTTP handlers.
//!
//! W0 landed `list` and `get` as a stub to reserve the route namespace.
//! W1 extends this with full CRUD (`create` / `update` / `delete`) plus the
//! `list_relations` endpoint that returns the in-and-out edges for a single
//! entity. Tenant isolation follows the same `is_super_admin` pattern used
//! by the Issue handler.

use axum::{
    Json,
    extract::{Path, State},
};
use uuid::Uuid;

use crate::AppState;
use crate::error::{AppError, AppResult};
use crate::middleware::auth::AuthUser;
use crate::models::catalog::{
    CatalogEntity, CatalogRelation, CreateEntityRequest, LIFECYCLE_EXPERIMENTAL,
    UpdateEntityRequest,
};

/// GET /api/catalog/entities
pub async fn list(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
) -> AppResult<Json<Vec<CatalogEntity>>> {
    let rows = if auth_user.is_super_admin() {
        sqlx::query_as::<_, CatalogEntity>(
            r#"SELECT * FROM catalog_entities
               ORDER BY created_at DESC
               LIMIT 500"#,
        )
        .fetch_all(&state.pool)
        .await?
    } else {
        sqlx::query_as::<_, CatalogEntity>(
            r#"SELECT * FROM catalog_entities
               WHERE tenant_id = $1
               ORDER BY created_at DESC
               LIMIT 500"#,
        )
        .bind(auth_user.tenant_id)
        .fetch_all(&state.pool)
        .await?
    };
    Ok(Json(rows))
}

/// GET /api/catalog/entities/{id}
pub async fn get(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<CatalogEntity>> {
    let row: Option<CatalogEntity> = if auth_user.is_super_admin() {
        sqlx::query_as::<_, CatalogEntity>(r#"SELECT * FROM catalog_entities WHERE id = $1"#)
            .bind(id)
            .fetch_optional(&state.pool)
            .await?
    } else {
        sqlx::query_as::<_, CatalogEntity>(
            r#"SELECT * FROM catalog_entities WHERE id = $1 AND tenant_id = $2"#,
        )
        .bind(id)
        .bind(auth_user.tenant_id)
        .fetch_optional(&state.pool)
        .await?
    };

    row.map(Json)
        .ok_or_else(|| AppError::NotFound(format!("Catalog entity not found: {}", id)))
}

/// POST /api/catalog/entities
pub async fn create(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Json(req): Json<CreateEntityRequest>,
) -> AppResult<Json<CatalogEntity>> {
    // Validate kind against the same enum the CHECK constraint enforces.
    if !CatalogEntity::is_valid_kind(&req.kind) {
        return Err(AppError::BadRequest(format!(
            "Invalid kind: {} (expected system|component|api|resource|group)",
            req.kind
        )));
    }

    let lifecycle = req
        .lifecycle
        .as_deref()
        .unwrap_or(LIFECYCLE_EXPERIMENTAL)
        .to_string();
    if !CatalogEntity::is_valid_lifecycle(&lifecycle) {
        return Err(AppError::BadRequest(format!(
            "Invalid lifecycle: {} (expected production|experimental|deprecated|retired)",
            lifecycle
        )));
    }

    // Non–super-admin callers must own a tenant; `catalog_entities.tenant_id`
    // is NOT NULL and belongs to the caller's tenant. Super-admin create is
    // deferred until a cross-tenant UX exists — reject it here for now.
    let tenant_id = auth_user.tenant_id.ok_or_else(|| {
        AppError::Forbidden("Tenant scope required to create catalog entity".to_string())
    })?;

    let row = sqlx::query_as::<_, CatalogEntity>(
        r#"INSERT INTO catalog_entities (
               tenant_id, kind, name, display_name, description, lifecycle,
               owner_group_id, system_id, tags, annotations,
               source_url, source_ref, spec
           )
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
           RETURNING *"#,
    )
    .bind(tenant_id)
    .bind(&req.kind)
    .bind(&req.name)
    .bind(&req.display_name)
    .bind(&req.description)
    .bind(&lifecycle)
    .bind(req.owner_group_id)
    .bind(req.system_id)
    .bind(&req.tags)
    .bind(&req.annotations)
    .bind(&req.source_url)
    .bind(&req.source_ref)
    .bind(&req.spec)
    .fetch_one(&state.pool)
    .await?;

    Ok(Json(row))
}

/// PUT /api/catalog/entities/{id}
pub async fn update(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateEntityRequest>,
) -> AppResult<Json<CatalogEntity>> {
    // Validate lifecycle upfront if the caller is trying to change it.
    if let Some(lifecycle) = req.lifecycle.as_deref() {
        if !CatalogEntity::is_valid_lifecycle(lifecycle) {
            return Err(AppError::BadRequest(format!(
                "Invalid lifecycle: {} (expected production|experimental|deprecated|retired)",
                lifecycle
            )));
        }
    }

    // Tenant isolation: fetch-then-check so the final UPDATE can run with
    // or without the tenant filter consistently.
    let existing =
        sqlx::query_as::<_, CatalogEntity>(r#"SELECT * FROM catalog_entities WHERE id = $1"#)
            .bind(id)
            .fetch_optional(&state.pool)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Catalog entity not found: {}", id)))?;
    if !auth_user.is_super_admin() && Some(existing.tenant_id) != auth_user.tenant_id {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }

    let row = sqlx::query_as::<_, CatalogEntity>(
        r#"UPDATE catalog_entities SET
               display_name    = COALESCE($2, display_name),
               description     = COALESCE($3, description),
               lifecycle       = COALESCE($4, lifecycle),
               owner_group_id  = COALESCE($5, owner_group_id),
               system_id       = COALESCE($6, system_id),
               tags            = COALESCE($7, tags),
               annotations     = COALESCE($8, annotations),
               source_url      = COALESCE($9, source_url),
               source_ref      = COALESCE($10, source_ref),
               spec            = COALESCE($11, spec),
               updated_at      = NOW()
           WHERE id = $1
           RETURNING *"#,
    )
    .bind(id)
    .bind(&req.display_name)
    .bind(&req.description)
    .bind(&req.lifecycle)
    .bind(req.owner_group_id)
    .bind(req.system_id)
    .bind(req.tags.as_deref())
    .bind(&req.annotations)
    .bind(&req.source_url)
    .bind(&req.source_ref)
    .bind(&req.spec)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Catalog entity not found: {}", id)))?;

    Ok(Json(row))
}

/// DELETE /api/catalog/entities/{id}
///
/// Cascades to `catalog_relations` via the ON DELETE CASCADE FK.
pub async fn delete(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let affected = if auth_user.is_super_admin() {
        sqlx::query("DELETE FROM catalog_entities WHERE id = $1")
            .bind(id)
            .execute(&state.pool)
            .await?
            .rows_affected()
    } else {
        let tenant_id = auth_user.tenant_id.ok_or_else(|| {
            AppError::Forbidden("Tenant scope required to delete catalog entity".to_string())
        })?;
        sqlx::query("DELETE FROM catalog_entities WHERE id = $1 AND tenant_id = $2")
            .bind(id)
            .bind(tenant_id)
            .execute(&state.pool)
            .await?
            .rows_affected()
    };

    if affected == 0 {
        return Err(AppError::NotFound(format!(
            "Catalog entity not found: {}",
            id
        )));
    }

    Ok(Json(serde_json::json!({ "message": "Deleted" })))
}

/// GET /api/catalog/entities/{id}/relations
///
/// Returns every relation where the given entity is either the `from_id`
/// or the `to_id`. Tenant isolation is enforced by first verifying that
/// the caller can see the entity itself.
pub async fn list_relations(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Vec<CatalogRelation>>> {
    // Reuse the same visibility logic as `get` — this both checks tenant
    // access and produces a clean 404 when the entity does not exist.
    let visible: Option<CatalogEntity> = if auth_user.is_super_admin() {
        sqlx::query_as::<_, CatalogEntity>(r#"SELECT * FROM catalog_entities WHERE id = $1"#)
            .bind(id)
            .fetch_optional(&state.pool)
            .await?
    } else {
        sqlx::query_as::<_, CatalogEntity>(
            r#"SELECT * FROM catalog_entities WHERE id = $1 AND tenant_id = $2"#,
        )
        .bind(id)
        .bind(auth_user.tenant_id)
        .fetch_optional(&state.pool)
        .await?
    };
    if visible.is_none() {
        return Err(AppError::NotFound(format!(
            "Catalog entity not found: {}",
            id
        )));
    }

    let rows = sqlx::query_as::<_, CatalogRelation>(
        r#"SELECT * FROM catalog_relations
           WHERE from_id = $1 OR to_id = $1
           ORDER BY created_at DESC"#,
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(rows))
}
