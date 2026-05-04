//! Catalog HTTP handlers.
//!
//! W0 landed `list` and `get` as a stub to reserve the route namespace.
//! W1 extends this with full CRUD (`create` / `update` / `delete`) plus the
//! `list_relations` endpoint that returns the in-and-out edges for a single
//! entity. Tenant isolation follows the same `is_super_admin` pattern used
//! by the Issue handler.

use axum::{
    Json,
    extract::{Path, Query, State},
};
use std::collections::HashSet;
use uuid::Uuid;

use crate::AppState;
use crate::error::{AppError, AppResult};
use crate::middleware::auth::AuthUser;
use crate::models::catalog::{
    CatalogEntity, CatalogRelation, CreateEntityRequest, DiscoverK8sRequest, DiscoverK8sResult,
    EntityGraph, GraphQuery, IMPORT_SOURCE_K8S_DISCOVERY, IMPORT_SOURCE_MANUAL, ImportYamlResult,
    KIND_COMPONENT, KIND_GROUP, KIND_SYSTEM, LIFECYCLE_EXPERIMENTAL, MAX_GRAPH_DEPTH,
    UpdateEntityRequest,
};
use crate::services::catalog::yaml_parser::{DeclaredRelation, RelationDirection};
use crate::services::catalog::{k8s_discovery, yaml_parser};

/// P2 #22: cursor-paginated list query. `after` is an exclusive upper
/// bound on `created_at` — the UI keeps fetching older pages by passing
/// the last row's `created_at` back in. `kind` optionally filters by
/// entity kind. `limit` defaults to 100 and is capped at 500.
#[derive(Debug, serde::Deserialize)]
pub struct ListEntitiesQuery {
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub after: Option<chrono::DateTime<chrono::Utc>>,
}

/// GET /api/catalog/entities
pub async fn list(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<ListEntitiesQuery>,
) -> AppResult<Json<Vec<CatalogEntity>>> {
    let limit = query.limit.unwrap_or(100).clamp(1, 500);

    let rows = if auth_user.is_super_admin() {
        sqlx::query_as::<_, CatalogEntity>(
            r#"SELECT * FROM catalog_entities
               WHERE ($1::TEXT IS NULL OR kind = $1)
                 AND ($2::TIMESTAMPTZ IS NULL OR created_at < $2)
               ORDER BY created_at DESC
               LIMIT $3"#,
        )
        .bind(query.kind.as_deref())
        .bind(query.after)
        .bind(limit)
        .fetch_all(&state.pool)
        .await?
    } else {
        sqlx::query_as::<_, CatalogEntity>(
            r#"SELECT * FROM catalog_entities
               WHERE tenant_id = $1
                 AND ($2::TEXT IS NULL OR kind = $2)
                 AND ($3::TIMESTAMPTZ IS NULL OR created_at < $3)
               ORDER BY created_at DESC
               LIMIT $4"#,
        )
        .bind(auth_user.tenant_id)
        .bind(query.kind.as_deref())
        .bind(query.after)
        .bind(limit)
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

    // Tenant isolation: the UPDATE itself carries the tenant predicate so a
    // race between `fetch_and_check`-style lookups and the write cannot
    // mutate another tenant's row. Super-admin callers bypass the filter;
    // anyone else must supply the tenant_id the row is expected to belong
    // to, and a mismatch cleanly returns NotFound.
    let row: Option<CatalogEntity> = if auth_user.is_super_admin() {
        sqlx::query_as::<_, CatalogEntity>(
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
    } else {
        let tenant_id = auth_user.tenant_id.ok_or_else(|| {
            AppError::Forbidden("Tenant scope required to update catalog entity".to_string())
        })?;
        sqlx::query_as::<_, CatalogEntity>(
            r#"UPDATE catalog_entities SET
                   display_name    = COALESCE($3, display_name),
                   description     = COALESCE($4, description),
                   lifecycle       = COALESCE($5, lifecycle),
                   owner_group_id  = COALESCE($6, owner_group_id),
                   system_id       = COALESCE($7, system_id),
                   tags            = COALESCE($8, tags),
                   annotations     = COALESCE($9, annotations),
                   source_url      = COALESCE($10, source_url),
                   source_ref      = COALESCE($11, source_ref),
                   spec            = COALESCE($12, spec),
                   updated_at      = NOW()
               WHERE id = $1 AND tenant_id = $2
               RETURNING *"#,
        )
        .bind(id)
        .bind(tenant_id)
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
    };

    let row = row.ok_or_else(|| AppError::NotFound(format!("Catalog entity not found: {}", id)))?;
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

/// GET /api/catalog/entities/{id}/graph?depth=2
///
/// Returns every entity reachable from the given one within `depth` hops
/// along `catalog_relations` edges (in either direction), plus the edges
/// themselves. Depth is clamped into `1..=MAX_GRAPH_DEPTH` so a runaway
/// query cannot walk the whole tenant catalog.
///
/// P1 #13: previously this was an iterative BFS in Rust issuing one
/// `SELECT * FROM catalog_relations WHERE from_id = $1 OR to_id = $1` per
/// visited node — a 3-hop walk across a dense catalog could trigger
/// hundreds of round trips. Now the traversal is a single recursive CTE
/// in Postgres, backed by the `idx_catalog_relations_from` /
/// `idx_catalog_relations_to` indexes. Tenant isolation is re-applied on
/// the node fetch the same way the old code did.
pub async fn get_graph(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<GraphQuery>,
) -> AppResult<Json<EntityGraph>> {
    // Normalise depth: reject nonsense + hard-cap at MAX_GRAPH_DEPTH.
    let depth = query.depth.clamp(1, MAX_GRAPH_DEPTH);

    // Start by loading the center node through the same visibility rule as
    // `get` — this also produces a clean 404 when the entity does not exist
    // and a 403 (via super-admin tenant filter) when caller can't see it.
    let center: CatalogEntity = if auth_user.is_super_admin() {
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
    }
    .ok_or_else(|| AppError::NotFound(format!("Catalog entity not found: {}", id)))?;

    let tenant_id = center.tenant_id;

    // One-shot recursive walk. `walk(id, depth)` starts at the center and
    // follows every edge in either direction until `depth < MAX_GRAPH_DEPTH`.
    // The outer join pulls every edge where both endpoints fall inside
    // `walk`, which is equivalent to the old `edges_by_id` collection.
    let edges_raw: Vec<CatalogRelation> = sqlx::query_as::<_, CatalogRelation>(
        r#"
        WITH RECURSIVE walk(id, depth) AS (
            SELECT $1::UUID, 0
            UNION
            SELECT CASE WHEN r.from_id = w.id THEN r.to_id ELSE r.from_id END,
                   w.depth + 1
            FROM catalog_relations r
            JOIN walk w ON (r.from_id = w.id OR r.to_id = w.id)
            WHERE w.depth < $2
        )
        SELECT DISTINCT r.*
        FROM catalog_relations r
        JOIN walk w1 ON w1.id = r.from_id
        JOIN walk w2 ON w2.id = r.to_id
        "#,
    )
    .bind(center.id)
    .bind(depth)
    .fetch_all(&state.pool)
    .await?;

    // Collect all ids we walked — the edges give us every endpoint reached
    // by the traversal. Seed with the center so an entity with zero edges
    // still renders.
    let mut node_id_set: HashSet<Uuid> = HashSet::new();
    node_id_set.insert(center.id);
    for e in &edges_raw {
        node_id_set.insert(e.from_id);
        node_id_set.insert(e.to_id);
    }
    let node_ids: Vec<Uuid> = node_id_set.into_iter().collect();

    // Fetch the actual node rows for the collected ids. Tenant isolation
    // is re-applied here — in theory all reachable nodes share the tenant
    // (the edges table cascades on entity delete), but belt-and-braces
    // keeps a corrupt edge from leaking cross-tenant data.
    let nodes: Vec<CatalogEntity> = if auth_user.is_super_admin() {
        sqlx::query_as::<_, CatalogEntity>(
            r#"SELECT * FROM catalog_entities WHERE id = ANY($1)"#,
        )
        .bind(&node_ids)
        .fetch_all(&state.pool)
        .await?
    } else {
        sqlx::query_as::<_, CatalogEntity>(
            r#"SELECT * FROM catalog_entities
               WHERE id = ANY($1) AND tenant_id = $2"#,
        )
        .bind(&node_ids)
        .bind(tenant_id)
        .fetch_all(&state.pool)
        .await?
    };

    // Drop any edges whose endpoints aren't in the filtered node set (can
    // happen in super-admin mode if a relation points to a node in a
    // different tenant). Keeps the ECharts-style graph self-consistent.
    let visible: HashSet<Uuid> = nodes.iter().map(|n| n.id).collect();
    let edges: Vec<CatalogRelation> = edges_raw
        .into_iter()
        .filter(|e| visible.contains(&e.from_id) && visible.contains(&e.to_id))
        .collect();

    Ok(Json(EntityGraph { nodes, edges }))
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
    // (kind, name) → id, populated during the upsert loop so the relation
    // resolution pass can short-circuit lookups for targets created in the
    // same import (e.g. `order-api` dependsOn `auth-service` where both
    // ship in one YAML).
    let mut entity_id_by_ref: std::collections::HashMap<(String, String), Uuid> =
        std::collections::HashMap::new();
    // (entity_id, declarations) — collected during upsert, resolved after
    // all entities are in place so forward references within the same
    // import file work.
    let mut pending_relations: Vec<(Uuid, String, Vec<DeclaredRelation>)> = Vec::new();

    // P1 #11: wrap the full import in a single transaction so a panic or
    // fatal DB error rolls everything back atomically. Per-entity failures
    // are still recorded in `errors` to preserve the partial-success
    // behaviour that operators expect — the transaction only rolls back on
    // truly unrecoverable errors (e.g. pool died, constraint violation on
    // the audit insert itself).
    let mut tx = state.pool.begin().await?;

    for entity in parsed {
        // Resolve owner group by name (if provided and resolvable).
        let owner_group_id: Option<Uuid> = match &entity.owner_group_name {
            Some(name) => sqlx::query_scalar::<_, Uuid>(
                r#"SELECT id FROM catalog_entities
                       WHERE tenant_id = $1 AND kind = $2 AND name = $3"#,
            )
            .bind(tenant_id)
            .bind(KIND_GROUP)
            .bind(name)
            .fetch_optional(&mut *tx)
            .await
            .unwrap_or(None),
            None => None,
        };

        // Resolve System reference by name.
        let system_id: Option<Uuid> = match &entity.system_name {
            Some(name) => sqlx::query_scalar::<_, Uuid>(
                r#"SELECT id FROM catalog_entities
                       WHERE tenant_id = $1 AND kind = $2 AND name = $3"#,
            )
            .bind(tenant_id)
            .bind(KIND_SYSTEM)
            .bind(name)
            .fetch_optional(&mut *tx)
            .await
            .unwrap_or(None),
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
        .fetch_optional(&mut *tx)
        .await?;

        // Capture the entity_id so we can ledger its declared_relations
        // for resolution once all entities are in place.
        let entity_id_opt: Option<Uuid> = if let Some(existing_id) = existing_id {
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
            .execute(&mut *tx)
            .await;

            match result {
                Ok(_) => {
                    updated += 1;
                    Some(existing_id)
                }
                Err(e) => {
                    errors.push(format!("{}/{}: {}", entity.kind, entity.name, e));
                    None
                }
            }
        } else {
            let inserted: Result<Uuid, _> = sqlx::query_scalar(
                r#"INSERT INTO catalog_entities (
                       tenant_id, kind, name, display_name, description, lifecycle,
                       owner_group_id, system_id, tags, annotations, spec
                   )
                   VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
                   RETURNING id"#,
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
            .fetch_one(&mut *tx)
            .await;

            match inserted {
                Ok(id) => {
                    created += 1;
                    Some(id)
                }
                Err(e) => {
                    errors.push(format!("{}/{}: {}", entity.kind, entity.name, e));
                    None
                }
            }
        };

        if let Some(eid) = entity_id_opt {
            entity_id_by_ref.insert((entity.kind.clone(), entity.name.clone()), eid);
            if !entity.declared_relations.is_empty() {
                pending_relations.push((eid, entity.kind.clone(), entity.declared_relations));
            }
        }
    }

    // ─── Resolve declared_relations → catalog_relations rows ──────────
    //
    // Design §4.2: for each relation declaration, look up the target
    // entity by `(tenant_id, name[, kind])` and INSERT ON CONFLICT DO
    // NOTHING so repeated imports are idempotent. Unresolved refs go
    // into `errors` (and eventually `catalog_import_runs.errors`), but
    // do not fail the import — missing cross-references should not
    // block progress on the entities that did resolve.
    let mut relations_written = 0i32;
    for (entity_id, entity_kind, declarations) in &pending_relations {
        for rel in declarations {
            // In-batch cache first (forward references inside this file).
            // Fall back to a DB lookup when the target wasn't part of
            // this import.
            let target_id: Option<Uuid> = if let Some(kind) = &rel.target_kind {
                // Kind hint present — exact lookup.
                let cached = entity_id_by_ref
                    .get(&(kind.clone(), rel.target_name.clone()))
                    .copied();
                if cached.is_some() {
                    cached
                } else {
                    sqlx::query_scalar::<_, Uuid>(
                        r#"SELECT id FROM catalog_entities
                           WHERE tenant_id = $1 AND kind = $2 AND name = $3"#,
                    )
                    .bind(tenant_id)
                    .bind(kind)
                    .bind(&rel.target_name)
                    .fetch_optional(&mut *tx)
                    .await
                    .unwrap_or(None)
                }
            } else {
                // No kind hint — match on name alone. Multiple rows with
                // the same name across kinds is a data-modelling mistake
                // we don't silently guess through.
                let rows: Vec<Uuid> = sqlx::query_scalar::<_, Uuid>(
                    r#"SELECT id FROM catalog_entities
                       WHERE tenant_id = $1 AND name = $2"#,
                )
                .bind(tenant_id)
                .bind(&rel.target_name)
                .fetch_all(&mut *tx)
                .await
                .unwrap_or_default();
                match rows.as_slice() {
                    [single] => Some(*single),
                    [] => None,
                    multi => {
                        errors.push(format!(
                            "{}/{}: ambiguous ref '{}' matches {} entities across kinds",
                            entity_kind,
                            rel.target_name,
                            rel.target_name,
                            multi.len()
                        ));
                        None
                    }
                }
            };

            let Some(target_id) = target_id else {
                errors.push(format!(
                    "{}: unresolved reference '{}'{}",
                    entity_kind,
                    rel.target_name,
                    rel.target_kind
                        .as_deref()
                        .map(|k| format!(" (kind={})", k))
                        .unwrap_or_default()
                ));
                continue;
            };

            // `FromEntity` → (entity, target); `ToEntity` → (target, entity).
            // Owner is the only reverse case today.
            let (from_id, to_id) = match rel.direction {
                RelationDirection::FromEntity => (*entity_id, target_id),
                RelationDirection::ToEntity => (target_id, *entity_id),
            };

            // Self-loops are never useful (e.g. a miscoded `dependsOn:
            // [self]`) — skip rather than clutter the table.
            if from_id == to_id {
                continue;
            }

            let insert_res = sqlx::query(
                r#"INSERT INTO catalog_relations (from_id, to_id, relation_type)
                   VALUES ($1, $2, $3)
                   ON CONFLICT (from_id, to_id, relation_type) DO NOTHING"#,
            )
            .bind(from_id)
            .bind(to_id)
            .bind(&rel.relation_type)
            .execute(&mut *tx)
            .await;

            match insert_res {
                Ok(r) if r.rows_affected() > 0 => relations_written += 1,
                Ok(_) => { /* already exists — idempotent no-op */ }
                Err(e) => errors.push(format!(
                    "{}/{}: failed to write {} relation: {}",
                    entity_kind, rel.target_name, rel.relation_type, e
                )),
            }
        }
    }
    // Surface relation count in logs so operators can sanity-check the
    // "catalog_relations had 0 rows" bug doesn't silently regress.
    tracing::info!(
        tenant_id = %tenant_id,
        relations_written,
        relations_attempted = pending_relations
            .iter()
            .map(|(_, _, r)| r.len())
            .sum::<usize>(),
        "catalog import relation pass finished"
    );

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
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;

    // W10: emit a change_events row so "what touched the catalog?" shows
    // up in the global stream alongside deploys and SLO burns. The service
    // linkage is intentionally null — an import acts on many entities at
    // once and the `catalog_import_runs.id` in the correlation_id lets a
    // curious operator drill in.
    crate::services::change_events::record_best_effort(
        &state.pool,
        Some(tenant_id),
        crate::models::change_event::KIND_CATALOG_IMPORT,
        None,
        serde_json::json!({
            "type": "user",
            "id": auth_user.user_id,
            "display_name": auth_user.username,
        }),
        format!("Imported {} entities, updated {}", created, updated),
        serde_json::json!({
            "run_id": run_id,
            "source": IMPORT_SOURCE_MANUAL,
            "entities_created": created,
            "entities_updated": updated,
        }),
        crate::models::change_event::SOURCE_IMPORT_RUN,
        Some(run_id.to_string()),
    )
    .await;

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

    // W10: mirror the yaml-import path into the global stream.
    crate::services::change_events::record_best_effort(
        &state.pool,
        Some(tenant_id),
        crate::models::change_event::KIND_CATALOG_IMPORT,
        None,
        serde_json::json!({
            "type": "user",
            "id": auth_user.user_id,
            "display_name": auth_user.username,
        }),
        format!(
            "Discovered {} entities, updated {} (cluster {})",
            created, updated, req.cluster_id
        ),
        serde_json::json!({
            "run_id": run_id,
            "source": IMPORT_SOURCE_K8S_DISCOVERY,
            "cluster_id": req.cluster_id,
            "entities_created": created,
            "entities_updated": updated,
        }),
        crate::models::change_event::SOURCE_IMPORT_RUN,
        Some(run_id.to_string()),
    )
    .await;

    Ok(Json(DiscoverK8sResult {
        run_id,
        entities_created: created,
        entities_updated: updated,
        errors,
    }))
}
