use uuid::Uuid;

use crate::error::AppError;
use crate::middleware::auth::AuthUser;

/// Map database constraint violations to user-friendly conflict errors.
///
/// `mappings` is a slice of (constraint_name, user_message) pairs.
/// If the error matches a known constraint, returns AppError::Conflict.
/// Otherwise, returns AppError::Database.
pub fn map_constraint_error(e: sqlx::Error, mappings: &[(&str, &str)]) -> AppError {
    if let sqlx::Error::Database(ref db_err) = e
        && let Some(constraint) = db_err.constraint()
    {
        for (name, message) in mappings {
            if constraint == *name {
                return AppError::Conflict(message.to_string());
            }
        }
    }
    AppError::Database(e)
}

/// Require the user to be a super_admin. Returns Forbidden if not.
pub fn require_super_admin(auth_user: &AuthUser, action: &str) -> Result<(), AppError> {
    if !auth_user.is_super_admin() {
        return Err(AppError::Forbidden(format!("Only super admins can {}", action)));
    }
    Ok(())
}

/// Require the string to be non-empty after trimming. Returns BadRequest if empty.
pub fn require_non_empty(value: &str, field_name: &str) -> Result<(), AppError> {
    if value.trim().is_empty() {
        return Err(AppError::BadRequest(format!("{} is required", field_name)));
    }
    Ok(())
}

/// Get the tenant_id filter value for list queries.
/// Returns None for super_admin (no filter), Some(id) for members.
pub fn tenant_filter(auth_user: &AuthUser) -> Option<Uuid> {
    if auth_user.is_super_admin() {
        None
    } else {
        auth_user.tenant_id
    }
}
