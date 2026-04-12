use axum::{
    Json,
    extract::{Path, State},
};
use uuid::Uuid;

use crate::AppState;
use crate::error::AppResult;
use crate::middleware::auth::AuthUser;
use crate::models::account_access::{
    AccessibleAccount, GrantAccessRequest, UserAccessView, UserAccountAccess,
};
use crate::services;

// Re-export for backward compatibility (used by other handlers)
pub use crate::services::account_access::{can_write_account, get_accessible_account_ids};

/// GET /api/my/accessible-accounts
pub async fn my_accessible_accounts(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
) -> AppResult<Json<Vec<AccessibleAccount>>> {
    let accounts =
        services::account_access::my_accessible_accounts(&state.pool, &auth_user).await?;
    Ok(Json(accounts))
}

/// GET /api/accounts/:id/users
pub async fn list_account_users(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(account_id): Path<Uuid>,
) -> AppResult<Json<Vec<UserAccessView>>> {
    let users =
        services::account_access::list_account_users(&state.pool, &auth_user, account_id).await?;
    Ok(Json(users))
}

/// POST /api/account-access/grant
pub async fn grant(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Json(req): Json<GrantAccessRequest>,
) -> AppResult<Json<UserAccountAccess>> {
    let access = services::account_access::grant(&state.pool, &auth_user, req).await?;
    Ok(Json(access))
}

/// DELETE /api/account-access/:user_id/:account_id
pub async fn revoke(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path((user_id, account_id)): Path<(Uuid, Uuid)>,
) -> AppResult<Json<serde_json::Value>> {
    services::account_access::revoke(&state.pool, &auth_user, user_id, account_id).await?;
    Ok(Json(serde_json::json!({"message": "Access revoked"})))
}
