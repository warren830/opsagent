//! Catalog HTTP handlers — W0 spec-lock stub.
//!
//! Only `list` and `get` are implemented here; full CRUD, import, and
//! discovery live in `feat/catalog-mvp`. The goal of this stub is to reserve
//! the route namespace (`/api/catalog/*`) and prove the table wiring.

use axum::{
    Json,
    extract::{Path, State},
};
use uuid::Uuid;

use crate::AppState;
use crate::error::{AppError, AppResult};
use crate::middleware::auth::AuthUser;
use crate::models::catalog::CatalogEntity;

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
