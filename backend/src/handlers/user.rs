use axum::{
    Json,
    extract::{Path, State},
};
use uuid::Uuid;

use crate::AppState;
use crate::error::AppResult;
use crate::middleware::auth::AuthUser;
use crate::models::user::{CreateUserRequest, InviteUserRequest, UpdateUserRequest, UserInfo};
use crate::services;

/// GET /api/users
pub async fn list_users(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
) -> AppResult<Json<Vec<UserInfo>>> {
    let users = services::user::list(&state.pool, &auth_user).await?;
    Ok(Json(users))
}

/// POST /api/users (super_admin only)
pub async fn create_user(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Json(req): Json<CreateUserRequest>,
) -> AppResult<Json<UserInfo>> {
    let user = services::user::create(&state.pool, &auth_user, req).await?;
    Ok(Json(user))
}

/// PUT /api/users/:id
pub async fn update_user(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateUserRequest>,
) -> AppResult<Json<UserInfo>> {
    let user = services::user::update(&state.pool, &auth_user, id, req).await?;
    Ok(Json(user))
}

/// DELETE /api/users/:id (super_admin only)
pub async fn delete_user(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    services::user::delete(&state.pool, &auth_user, id).await?;
    Ok(Json(serde_json::json!({"message": "User deleted"})))
}

/// POST /api/users/invite (super_admin only, cloud mode)
pub async fn invite_user(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Json(req): Json<InviteUserRequest>,
) -> AppResult<Json<UserInfo>> {
    let user = services::user::invite(&state.pool, &auth_user, req).await?;
    Ok(Json(user))
}
