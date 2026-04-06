use axum::{
    Json,
    extract::{Query, State},
};

use crate::AppState;
use crate::error::AppResult;
use crate::middleware::auth::AuthUser;
use crate::models::resource::{Resource, ResourceListQuery};

/// GET /api/resources
pub async fn list(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Query(query): Query<ResourceListQuery>,
) -> AppResult<Json<Vec<Resource>>> {
    let rows = if auth_user.is_super_admin() {
        sqlx::query_as::<_, Resource>(
            r#"SELECT * FROM resources
               WHERE ($1::TEXT IS NULL OR resource_type = $1)
                 AND ($2::TEXT IS NULL OR region = $2)
                 AND ($3::TEXT IS NULL OR LOWER(name) LIKE '%' || LOWER($3) || '%')
               ORDER BY created_at DESC"#,
        )
        .bind(&query.resource_type)
        .bind(&query.region)
        .bind(&query.q)
        .fetch_all(&state.pool)
        .await?
    } else {
        sqlx::query_as::<_, Resource>(
            r#"SELECT * FROM resources
               WHERE tenant_id = $1
                 AND ($2::TEXT IS NULL OR resource_type = $2)
                 AND ($3::TEXT IS NULL OR region = $3)
                 AND ($4::TEXT IS NULL OR LOWER(name) LIKE '%' || LOWER($4) || '%')
               ORDER BY created_at DESC"#,
        )
        .bind(auth_user.tenant_id)
        .bind(&query.resource_type)
        .bind(&query.region)
        .bind(&query.q)
        .fetch_all(&state.pool)
        .await?
    };
    Ok(Json(rows))
}

/// POST /api/resources/scan (mock)
pub async fn scan(_auth_user: axum::Extension<AuthUser>) -> AppResult<Json<serde_json::Value>> {
    Ok(Json(serde_json::json!({
        "status": "ok",
        "message": "Resource scan initiated"
    })))
}
