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
    CatalogEntity, CatalogRelation, CreateEntityRequest, DiscoverK8sRequest, DiscoverK8sResult,
    IMPORT_SOURCE_K8S_DISCOVERY, IMPORT_SOURCE_MANUAL, ImportYamlResult, KIND_COMPONENT,
    KIND_GROUP, KIND_SYSTEM, LIFECYCLE_EXPERIMENTAL, UpdateEntityRequest,
};
use crate::services::catalog::{k8s_discovery, yaml_parser};

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

/// POST /api/catalog/import/yaml
///
/// Body is the raw YAML text (one or more Backstage-style documents
/// separated by `---`). Each document becomes a `catalog_entities` row
/// keyed by `(tenant_id, kind, name)`; existing rows are updated in
/// place. `spec.owner = "group:xyz"` is resolved to `owner_group_id`
/// against an existing Group entity — missing groups fall back to
/// NULL (the handler doesn't auto-create them to keep import
/// idempotent).
///
/// An audit record is inserted into `catalog_import_runs` with
/// `source = 'manual'` regardless of success/failure of individual
/// entities so the operator can replay history.
pub async fn import_yaml(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    body: String,
) -> AppResult<Json<ImportYamlResult>> {
    let tenant_id = auth_user.tenant_id.ok_or_else(|| {
        AppError::Forbidden("Tenant scope required to import catalog entities".to_string())
    })?;

    let parsed = yaml_parser::parse_multi_doc(&body).map_err(|e| AppError::BadRequest(e.to_string()))?;

    let mut created = 0i32;
    let mut updated = 0i32;
    let mut errors: Vec<String> = Vec::new();

    for entity in parsed {
        // Resolve owner group by name (if provided and resolvable).
        let owner_group_id: Option<Uuid> = match &entity.owner_group_name {
            Some(name) => {
                sqlx::query_scalar::<_, Uuid>(
                    r#"SELECT id FROM catalog_entities
                       WHERE tenant_id = $1 AND kind = $2 AND name = $3"#,
                )
                .bind(tenant_id)
                .bind(KIND_GROUP)
                .bind(name)
                .fetch_optional(&state.pool)
                .await
                .unwrap_or(None)
            }
            None => None,
        };

        // Resolve System reference by name.
        let system_id: Option<Uuid> = match &entity.system_name {
            Some(name) => {
                sqlx::query_scalar::<_, Uuid>(
                    r#"SELECT id FROM catalog_entities
                       WHERE tenant_id = $1 AND kind = $2 AND name = $3"#,
                )
                .bind(tenant_id)
                .bind(KIND_SYSTEM)
                .bind(name)
                .fetch_optional(&state.pool)
                .await
                .unwrap_or(None)
            }
            None => None,
        };

        // Is there an existing row to update?
        let existing_id: Option<Uuid> = sqlx::query_scalar(
            r#"SELECT id FROM catalog_entities
               WHERE tenant_id = $1 AND kind = $2 AND name = $3"#,
        )
        .bind(tenant_id)
        .bind(&entity.kind)
        .bind(&entity.name)
        .fetch_optional(&state.pool)
        .await?;

        if let Some(existing_id) = existing_id {
            let result = sqlx::query(
                r#"UPDATE catalog_entities SET
                       display_name   = $2,
                       description    = $3,
                       lifecycle      = $4,
                       owner_group_id = $5,
                       system_id      = $6,
                       tags           = $7,
                       annotations    = $8,
                       spec           = $9,
                       updated_at     = NOW()
                   WHERE id = $1"#,
            )
            .bind(existing_id)
            .bind(&entity.display_name)
            .bind(&entity.description)
            .bind(&entity.lifecycle)
            .bind(owner_group_id)
            .bind(system_id)
            .bind(&entity.tags)
            .bind(&entity.annotations)
            .bind(&entity.spec_remaining)
            .execute(&state.pool)
            .await;

            match result {
                Ok(_) => updated += 1,
                Err(e) => errors.push(format!("{}/{}: {}", entity.kind, entity.name, e)),
            }
        } else {
            let result = sqlx::query(
                r#"INSERT INTO catalog_entities (
                       tenant_id, kind, name, display_name, description, lifecycle,
                       owner_group_id, system_id, tags, annotations, spec
                   )
                   VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)"#,
            )
            .bind(tenant_id)
            .bind(&entity.kind)
            .bind(&entity.name)
            .bind(&entity.display_name)
            .bind(&entity.description)
            .bind(&entity.lifecycle)
            .bind(owner_group_id)
            .bind(system_id)
            .bind(&entity.tags)
            .bind(&entity.annotations)
            .bind(&entity.spec_remaining)
            .execute(&state.pool)
            .await;

            match result {
                Ok(_) => created += 1,
                Err(e) => errors.push(format!("{}/{}: {}", entity.kind, entity.name, e)),
            }
        }
    }

    // Audit record — `completed_at` is set now so this row represents a
    // single-shot synchronous import. Errors are serialised as a JSON
    // array so downstream tooling can parse per-entry failures.
    let errors_json = serde_json::to_value(&errors).unwrap_or(serde_json::json!([]));
    let run_id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO catalog_import_runs (
               tenant_id, source, entities_created, entities_updated, errors, completed_at
           )
           VALUES ($1, $2, $3, $4, $5, NOW())
           RETURNING id"#,
    )
    .bind(tenant_id)
    .bind(IMPORT_SOURCE_MANUAL)
    .bind(created)
    .bind(updated)
    .bind(&errors_json)
    .fetch_one(&state.pool)
    .await?;

    Ok(Json(ImportYamlResult {
        run_id,
        entities_created: created,
        entities_updated: updated,
        errors,
    }))
}

/// POST /api/catalog/discover/k8s
///
/// Walks a cluster and creates placeholder `Component` entities for
/// every distinct `app.kubernetes.io/name` value (fallback to workload
/// name). Existing Components are refreshed only on their
/// `spec.runtime` block — we never clobber human-curated fields like
/// `lifecycle` or `owner_group_id` during discovery.
pub async fn discover_k8s(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Json(req): Json<DiscoverK8sRequest>,
) -> AppResult<Json<DiscoverK8sResult>> {
    let tenant_id = auth_user.tenant_id.ok_or_else(|| {
        AppError::Forbidden("Tenant scope required to run discovery".to_string())
    })?;

    // Tenant check — super-admin can scan any cluster, everyone else is
    // scoped to their own tenant (including cluster rows with NULL
    // tenant_id, which are treated as tenant-agnostic shared clusters).
    let cluster = sqlx::query_as::<_, crate::models::cluster::Cluster>(
        "SELECT * FROM clusters WHERE id = $1",
    )
    .bind(req.cluster_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Cluster not found: {}", req.cluster_id)))?;

    if !auth_user.is_super_admin()
        && cluster.tenant_id.is_some()
        && cluster.tenant_id != Some(tenant_id)
    {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }

    let result = k8s_discovery::discover_cluster(&state.pool, req.cluster_id).await?;
    let mut created = 0i32;
    let mut updated = 0i32;
    let mut errors: Vec<String> = result.errors.clone();

    for comp in &result.discovered {
        let runtime_json = serde_json::json!({
            "runtime": {
                "kind": "eks",
                "cluster_id": comp.cluster_id,
                "namespace": comp.namespace,
                "workload": comp.workload_name,
            }
        });

        let existing_id: Option<Uuid> = sqlx::query_scalar(
            r#"SELECT id FROM catalog_entities
               WHERE tenant_id = $1 AND kind = $2 AND name = $3"#,
        )
        .bind(tenant_id)
        .bind(KIND_COMPONENT)
        .bind(&comp.name)
        .fetch_optional(&state.pool)
        .await?;

        if let Some(existing_id) = existing_id {
            // Merge runtime into existing spec — jsonb || overrides keys.
            let result = sqlx::query(
                r#"UPDATE catalog_entities SET
                       spec       = COALESCE(spec, '{}'::jsonb) || $2::jsonb,
                       updated_at = NOW()
                   WHERE id = $1"#,
            )
            .bind(existing_id)
            .bind(&runtime_json)
            .execute(&state.pool)
            .await;

            match result {
                Ok(_) => updated += 1,
                Err(e) => errors.push(format!("component/{}: {}", comp.name, e)),
            }
        } else {
            // Fresh placeholder — lifecycle=experimental, owner NULL.
            let result = sqlx::query(
                r#"INSERT INTO catalog_entities (
                       tenant_id, kind, name, display_name, lifecycle,
                       tags, annotations, spec
                   )
                   VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#,
            )
            .bind(tenant_id)
            .bind(KIND_COMPONENT)
            .bind(&comp.name)
            .bind(&comp.name)
            .bind(LIFECYCLE_EXPERIMENTAL)
            .bind::<&[String]>(&[])
            .bind(serde_json::json!({}))
            .bind(&runtime_json)
            .execute(&state.pool)
            .await;

            match result {
                Ok(_) => created += 1,
                Err(e) => errors.push(format!("component/{}: {}", comp.name, e)),
            }
        }
    }

    let errors_json = serde_json::to_value(&errors).unwrap_or(serde_json::json!([]));
    let run_id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO catalog_import_runs (
               tenant_id, source, source_ref, entities_created, entities_updated, errors, completed_at
           )
           VALUES ($1, $2, $3, $4, $5, $6, NOW())
           RETURNING id"#,
    )
    .bind(tenant_id)
    .bind(IMPORT_SOURCE_K8S_DISCOVERY)
    .bind(req.cluster_id.to_string())
    .bind(created)
    .bind(updated)
    .bind(&errors_json)
    .fetch_one(&state.pool)
    .await?;

    Ok(Json(DiscoverK8sResult {
        run_id,
        entities_created: created,
        entities_updated: updated,
        errors,
    }))
}
