//! Catalog entity types — W0 spec-lock stub.
//!
//! This module defines the minimum viable `CatalogEntity` struct so that
//! SLO and Incident modules can start taking `component_id` (UUID) as a
//! stable foreign key while the full Catalog features are implemented in
//! `feat/catalog-mvp`.
//!
//! See:
//! - `docs/platform-evolution.md` §三 for full Catalog design.
//! - `.claude/plans/plan-tingly-cherny.md` for the worktree roadmap.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// Valid `kind` values (mirrors the CHECK constraint in migration
/// `20260502000001_component_spec_lock.sql`).
pub const KIND_SYSTEM: &str = "system";
pub const KIND_COMPONENT: &str = "component";
pub const KIND_API: &str = "api";
pub const KIND_RESOURCE: &str = "resource";
pub const KIND_GROUP: &str = "group";

pub const ALL_KINDS: &[&str] = &[
    KIND_SYSTEM,
    KIND_COMPONENT,
    KIND_API,
    KIND_RESOURCE,
    KIND_GROUP,
];

/// Valid `lifecycle` values (mirrors the CHECK constraint).
pub const LIFECYCLE_PRODUCTION: &str = "production";
pub const LIFECYCLE_EXPERIMENTAL: &str = "experimental";
pub const LIFECYCLE_DEPRECATED: &str = "deprecated";
pub const LIFECYCLE_RETIRED: &str = "retired";

pub const ALL_LIFECYCLES: &[&str] = &[
    LIFECYCLE_PRODUCTION,
    LIFECYCLE_EXPERIMENTAL,
    LIFECYCLE_DEPRECATED,
    LIFECYCLE_RETIRED,
];

/// Single-table entity record. The `kind` column discriminates between
/// System / Component / API / Resource / Group; kind-specific fields live
/// inside `spec` (JSONB) so the schema stays stable as the Catalog evolves.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CatalogEntity {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub kind: String,
    pub name: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub lifecycle: String,
    pub owner_group_id: Option<Uuid>,
    pub system_id: Option<Uuid>,
    pub tags: Vec<String>,
    pub annotations: serde_json::Value,
    pub source_url: Option<String>,
    pub source_ref: Option<String>,
    pub spec: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl CatalogEntity {
    /// Returns true if the given `kind` value is one of the five valid kinds.
    pub fn is_valid_kind(kind: &str) -> bool {
        ALL_KINDS.contains(&kind)
    }

    /// Returns true if the given `lifecycle` value is valid.
    pub fn is_valid_lifecycle(lifecycle: &str) -> bool {
        ALL_LIFECYCLES.contains(&lifecycle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_validation_accepts_all_five_kinds() {
        assert!(CatalogEntity::is_valid_kind(KIND_SYSTEM));
        assert!(CatalogEntity::is_valid_kind(KIND_COMPONENT));
        assert!(CatalogEntity::is_valid_kind(KIND_API));
        assert!(CatalogEntity::is_valid_kind(KIND_RESOURCE));
        assert!(CatalogEntity::is_valid_kind(KIND_GROUP));
    }

    #[test]
    fn kind_validation_rejects_unknown_values() {
        assert!(!CatalogEntity::is_valid_kind("service"));
        assert!(!CatalogEntity::is_valid_kind(""));
        assert!(!CatalogEntity::is_valid_kind("SYSTEM"));
    }

    #[test]
    fn lifecycle_validation_accepts_all_four() {
        assert!(CatalogEntity::is_valid_lifecycle(LIFECYCLE_PRODUCTION));
        assert!(CatalogEntity::is_valid_lifecycle(LIFECYCLE_EXPERIMENTAL));
        assert!(CatalogEntity::is_valid_lifecycle(LIFECYCLE_DEPRECATED));
        assert!(CatalogEntity::is_valid_lifecycle(LIFECYCLE_RETIRED));
    }

    #[test]
    fn serde_roundtrip_preserves_all_fields() {
        let original = CatalogEntity {
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            kind: KIND_COMPONENT.to_string(),
            name: "order-api".to_string(),
            display_name: Some("Order API".to_string()),
            description: Some("Order REST service".to_string()),
            lifecycle: LIFECYCLE_PRODUCTION.to_string(),
            owner_group_id: None,
            system_id: None,
            tags: vec!["rust".into(), "critical".into()],
            annotations: serde_json::json!({"github.com/slug": "acme/order-api"}),
            source_url: Some("https://github.com/acme/order-api".into()),
            source_ref: Some("main@abc123".into()),
            spec: serde_json::json!({"runtime": {"kind": "eks"}}),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let json = serde_json::to_string(&original).expect("serialize");
        let parsed: CatalogEntity = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(parsed.id, original.id);
        assert_eq!(parsed.kind, original.kind);
        assert_eq!(parsed.name, original.name);
        assert_eq!(parsed.tags.len(), 2);
        assert_eq!(parsed.annotations, original.annotations);
    }
}
