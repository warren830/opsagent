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

/// Valid `relation_type` values (mirrors the CHECK constraint in migration
/// `20260503000001_catalog_relations.sql`).
pub const RELATION_OWNS: &str = "owns";
pub const RELATION_PROVIDES: &str = "provides";
pub const RELATION_CONSUMES: &str = "consumes";
pub const RELATION_DEPENDS_ON: &str = "depends_on";
pub const RELATION_PART_OF: &str = "part_of";
pub const RELATION_DEPLOYED_ON: &str = "deployed_on";

pub const ALL_RELATION_TYPES: &[&str] = &[
    RELATION_OWNS,
    RELATION_PROVIDES,
    RELATION_CONSUMES,
    RELATION_DEPENDS_ON,
    RELATION_PART_OF,
    RELATION_DEPLOYED_ON,
];

/// Valid `role` values for group membership.
pub const ROLE_OWNER: &str = "owner";
pub const ROLE_MEMBER: &str = "member";

pub const ALL_ROLES: &[&str] = &[ROLE_OWNER, ROLE_MEMBER];

/// Valid `source` values for catalog import runs.
pub const IMPORT_SOURCE_MANUAL: &str = "manual";
pub const IMPORT_SOURCE_GITHUB_ORG: &str = "github_org";
pub const IMPORT_SOURCE_GIT_URL: &str = "git_url";
pub const IMPORT_SOURCE_K8S_DISCOVERY: &str = "k8s_discovery";

pub const ALL_IMPORT_SOURCES: &[&str] = &[
    IMPORT_SOURCE_MANUAL,
    IMPORT_SOURCE_GITHUB_ORG,
    IMPORT_SOURCE_GIT_URL,
    IMPORT_SOURCE_K8S_DISCOVERY,
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

/// Request payload for `POST /api/catalog/entities`.
///
/// `lifecycle` is optional at the wire level; the handler falls back to
/// `"experimental"` when absent to match the database column default.
#[derive(Debug, Deserialize)]
pub struct CreateEntityRequest {
    pub kind: String,
    pub name: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub lifecycle: Option<String>,
    pub owner_group_id: Option<Uuid>,
    pub system_id: Option<Uuid>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub annotations: serde_json::Value,
    pub source_url: Option<String>,
    pub source_ref: Option<String>,
    #[serde(default)]
    pub spec: serde_json::Value,
}

/// Request payload for `PUT /api/catalog/entities/{id}`.
///
/// Every field is optional; the handler applies partial updates using
/// `COALESCE($N, column)`. `kind`, `tenant_id`, and immutable audit fields
/// are intentionally omitted — renaming a kind is not supported via update.
#[derive(Debug, Deserialize)]
pub struct UpdateEntityRequest {
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub lifecycle: Option<String>,
    pub owner_group_id: Option<Uuid>,
    pub system_id: Option<Uuid>,
    pub tags: Option<Vec<String>>,
    pub annotations: Option<serde_json::Value>,
    pub source_url: Option<String>,
    pub source_ref: Option<String>,
    pub spec: Option<serde_json::Value>,
}

/// Typed edge between two `catalog_entities`. The combination
/// (`from_id`, `to_id`, `relation_type`) is unique; deletion of either
/// endpoint cascades to the relation row.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CatalogRelation {
    pub id: Uuid,
    pub from_id: Uuid,
    pub to_id: Uuid,
    pub relation_type: String,
    pub created_at: DateTime<Utc>,
}

impl CatalogRelation {
    /// Returns true if the given `relation_type` value is one of the six
    /// valid types.
    pub fn is_valid_relation_type(relation_type: &str) -> bool {
        ALL_RELATION_TYPES.contains(&relation_type)
    }
}

/// Membership link between a Group entity (`catalog_entities.kind = 'group'`)
/// and a platform `users` row. Primary key is the composite
/// (`group_id`, `user_id`).
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CatalogGroupMember {
    pub group_id: Uuid,
    pub user_id: Uuid,
    pub role: String,
}

impl CatalogGroupMember {
    /// Returns true if the given `role` value is valid.
    pub fn is_valid_role(role: &str) -> bool {
        ALL_ROLES.contains(&role)
    }
}

/// Audit record of a single catalog import run (YAML upload, GitHub org
/// sync, K8s discovery pass, etc.). `errors` is a JSON array of per-entity
/// failure descriptors; `completed_at` is NULL while the run is in flight.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CatalogImportRun {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub source: String,
    pub source_ref: Option<String>,
    pub entities_created: i32,
    pub entities_updated: i32,
    pub errors: serde_json::Value,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

impl CatalogImportRun {
    /// Returns true if the given `source` value is one of the four supported
    /// import sources.
    pub fn is_valid_source(source: &str) -> bool {
        ALL_IMPORT_SOURCES.contains(&source)
    }
}

/// Request payload for `POST /api/catalog/discover/k8s`.
#[derive(Debug, Deserialize)]
pub struct DiscoverK8sRequest {
    pub cluster_id: Uuid,
}

/// Response shape shared by both import endpoints. `run_id` points to
/// the `catalog_import_runs` audit record; `errors` is a best-effort
/// list of per-entity failure messages so callers can surface partial
/// success in the UI.
#[derive(Debug, Serialize)]
pub struct ImportYamlResult {
    pub run_id: Uuid,
    pub entities_created: i32,
    pub entities_updated: i32,
    pub errors: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct DiscoverK8sResult {
    pub run_id: Uuid,
    pub entities_created: i32,
    pub entities_updated: i32,
    pub errors: Vec<String>,
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

    #[test]
    fn relation_type_validation_accepts_all_six_types() {
        assert!(CatalogRelation::is_valid_relation_type(RELATION_OWNS));
        assert!(CatalogRelation::is_valid_relation_type(RELATION_PROVIDES));
        assert!(CatalogRelation::is_valid_relation_type(RELATION_CONSUMES));
        assert!(CatalogRelation::is_valid_relation_type(RELATION_DEPENDS_ON));
        assert!(CatalogRelation::is_valid_relation_type(RELATION_PART_OF));
        assert!(CatalogRelation::is_valid_relation_type(RELATION_DEPLOYED_ON));

        assert!(!CatalogRelation::is_valid_relation_type("depends-on"));
        assert!(!CatalogRelation::is_valid_relation_type(""));
        assert!(!CatalogRelation::is_valid_relation_type("OWNS"));
    }

    #[test]
    fn role_validation_accepts_owner_and_member() {
        assert!(CatalogGroupMember::is_valid_role(ROLE_OWNER));
        assert!(CatalogGroupMember::is_valid_role(ROLE_MEMBER));

        assert!(!CatalogGroupMember::is_valid_role("admin"));
        assert!(!CatalogGroupMember::is_valid_role(""));
        assert!(!CatalogGroupMember::is_valid_role("Owner"));
    }

    #[test]
    fn catalog_relation_serde_roundtrip() {
        let original = CatalogRelation {
            id: Uuid::new_v4(),
            from_id: Uuid::new_v4(),
            to_id: Uuid::new_v4(),
            relation_type: RELATION_DEPENDS_ON.to_string(),
            created_at: Utc::now(),
        };

        let json = serde_json::to_string(&original).expect("serialize");
        let parsed: CatalogRelation = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(parsed.id, original.id);
        assert_eq!(parsed.from_id, original.from_id);
        assert_eq!(parsed.to_id, original.to_id);
        assert_eq!(parsed.relation_type, RELATION_DEPENDS_ON);
    }

    #[test]
    fn catalog_import_run_serde_roundtrip() {
        let original = CatalogImportRun {
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            source: IMPORT_SOURCE_GITHUB_ORG.to_string(),
            source_ref: Some("acme/backstage-catalog".into()),
            entities_created: 12,
            entities_updated: 3,
            errors: serde_json::json!([
                {"name": "broken-service", "reason": "missing kind"}
            ]),
            started_at: Utc::now(),
            completed_at: Some(Utc::now()),
        };

        let json = serde_json::to_string(&original).expect("serialize");
        let parsed: CatalogImportRun = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(parsed.id, original.id);
        assert_eq!(parsed.tenant_id, original.tenant_id);
        assert_eq!(parsed.source, IMPORT_SOURCE_GITHUB_ORG);
        assert_eq!(parsed.entities_created, 12);
        assert_eq!(parsed.entities_updated, 3);
        assert_eq!(parsed.errors, original.errors);
        assert!(parsed.completed_at.is_some());

        assert!(CatalogImportRun::is_valid_source(IMPORT_SOURCE_MANUAL));
        assert!(CatalogImportRun::is_valid_source(IMPORT_SOURCE_K8S_DISCOVERY));
        assert!(!CatalogImportRun::is_valid_source("csv"));
    }
}
