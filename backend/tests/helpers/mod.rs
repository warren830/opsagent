pub mod seed;
pub mod http;

use uuid::Uuid;

/// A minimal representation of AuthUser for testing service functions.
/// Mirrors the fields from `loops::middleware::auth::AuthUser`.
#[derive(Debug, Clone)]
pub struct TestAuthUser {
    pub user_id: Uuid,
    pub role: String,
    pub tenant_id: Option<Uuid>,
    pub username: String,
}

impl TestAuthUser {
    pub fn super_admin() -> loops::middleware::auth::AuthUser {
        loops::middleware::auth::AuthUser {
            user_id: Uuid::new_v4(),
            role: "super_admin".to_string(),
            tenant_id: None,
            username: "test_admin".to_string(),
        }
    }

    pub fn member(tenant_id: Uuid) -> loops::middleware::auth::AuthUser {
        loops::middleware::auth::AuthUser {
            user_id: Uuid::new_v4(),
            role: "member".to_string(),
            tenant_id: Some(tenant_id),
            username: "test_member".to_string(),
        }
    }

    pub fn tenant_admin(tenant_id: Uuid) -> loops::middleware::auth::AuthUser {
        loops::middleware::auth::AuthUser {
            user_id: Uuid::new_v4(),
            role: "tenant_admin".to_string(),
            tenant_id: Some(tenant_id),
            username: "test_tenant_admin".to_string(),
        }
    }
}
