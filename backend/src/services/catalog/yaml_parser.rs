//! Parser for Backstage-style `catalog-info.yaml` documents.
//!
//! Accepts a multi-document YAML stream (documents separated by `---`)
//! and returns a `Vec<ParsedEntity>`. Each document must have
//! `apiVersion: loops.yingchu.cloud/v1` and `kind` in
//! `System | Component | API | Resource | Group`.
//!
//! The parser pulls a few well-known fields out of `spec` (`lifecycle`,
//! `owner`, `system`) into typed fields on `ParsedEntity` so the handler
//! can look up group / system FKs without rummaging through JSON, and
//! keeps everything else in `spec_remaining` for storage in the
//! `catalog_entities.spec` JSONB column.
//!
//! In addition to the typed fields, the parser extracts a list of
//! `DeclaredRelation`s per entity from `spec.system`, `spec.owner`,
//! `spec.dependsOn`, `spec.providesApis`, and `spec.consumesApis`. The
//! handler resolves each reference to an entity id post-upsert and
//! inserts edges into `catalog_relations` (see design
//! `2026-05-03-services-v2-multi-runtime` §4.2).

use crate::models::catalog::{
    RELATION_CONSUMES, RELATION_DEPENDS_ON, RELATION_OWNS, RELATION_PART_OF, RELATION_PROVIDES,
};
use serde::Deserialize;

const EXPECTED_API_VERSION: &str = "loops.yingchu.cloud/v1";

/// P1 #11: cap the raw YAML payload size. 5 MB is ~50k realistic entities
/// which is an order of magnitude more than any catalog we expect. Anything
/// larger is almost certainly a YAML-bomb / DoS attempt.
const MAX_YAML_BYTES: usize = 5 * 1024 * 1024;
/// P1 #11: cap the total number of anchor (`&`) and alias (`*`) markers.
/// Classic billion-laughs requires recursive alias expansion; refuse
/// anything above 100 markers to keep the attack surface tiny.
const MAX_YAML_ALIAS_MARKERS: usize = 100;
/// P1 #11: cap the number of documents in a single request. Realistic
/// catalog-info bundles are well under 1000 entities; anything more is a
/// client bug or an attack.
const MAX_YAML_DOCS: usize = 1000;

/// Raw YAML document shape — mirrors the wire format. Everything after
/// the standard metadata block goes into `spec` as opaque JSON.
#[derive(Debug, Deserialize)]
pub struct CatalogYamlDoc {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub kind: String,
    pub metadata: YamlMetadata,
    #[serde(default)]
    pub spec: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct YamlMetadata {
    pub name: String,
    #[serde(rename = "displayName")]
    pub display_name: Option<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub annotations: serde_json::Value,
}

/// Normalised entity ready to be upserted into `catalog_entities`.
/// `kind` is lower-cased to match the DB CHECK constraint; the owner
/// and system references are name strings that the handler resolves to
/// UUIDs.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedEntity {
    pub kind: String,
    pub name: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub lifecycle: String,
    pub owner_group_name: Option<String>,
    pub system_name: Option<String>,
    pub tags: Vec<String>,
    pub annotations: serde_json::Value,
    pub spec_remaining: serde_json::Value,
    /// Relation declarations extracted from this entity's spec. Handler
    /// resolves `target_name` (+ optional `target_kind`) to a
    /// `catalog_entities.id` post-upsert and inserts into
    /// `catalog_relations`.
    pub declared_relations: Vec<DeclaredRelation>,
}

/// A single relation declaration pending resolution. `direction` decides
/// whether the current entity is the `from_id` or the `to_id`; the
/// `owner` case is the only reverse direction (owner → entity).
#[derive(Debug, Clone, PartialEq)]
pub struct DeclaredRelation {
    /// Bare name of the target entity (stripped of any `kind:` prefix).
    pub target_name: String,
    /// Optional hint at the target's `kind` (`component`, `api`,
    /// `resource`, `system`, `group`). `None` means "look up by name
    /// across any kind". The handler narrows this when more than one
    /// entity shares a name.
    pub target_kind: Option<String>,
    /// One of `catalog_relations` relation_type values.
    pub relation_type: String,
    /// `FromEntity` → relation goes (entity, target); `ToEntity` →
    /// relation goes (target, entity). Only `owner` uses the reverse
    /// direction today.
    pub direction: RelationDirection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationDirection {
    /// Current entity is `from_id`; `target_name` resolves to `to_id`.
    FromEntity,
    /// Current entity is `to_id`; `target_name` resolves to `from_id`.
    /// Used for `owner` (owner owns entity).
    ToEntity,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParseError {
    InvalidYaml(String),
    InvalidApiVersion(String),
    InvalidKind(String),
    MissingName,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::InvalidYaml(m) => write!(f, "invalid YAML: {m}"),
            ParseError::InvalidApiVersion(v) => {
                write!(f, "invalid apiVersion: '{v}' (expected '{EXPECTED_API_VERSION}')")
            }
            ParseError::InvalidKind(k) => write!(
                f,
                "invalid kind: '{k}' (expected System|Component|API|Resource|Group)"
            ),
            ParseError::MissingName => write!(f, "metadata.name is required"),
        }
    }
}

impl std::error::Error for ParseError {}

/// Normalise `kind` from the YAML casing to the DB form.
/// Returns `None` if the kind is not one of the five supported values.
fn normalise_kind(raw: &str) -> Option<&'static str> {
    match raw {
        "System" => Some("system"),
        "Component" => Some("component"),
        "API" => Some("api"),
        "Resource" => Some("resource"),
        "Group" => Some("group"),
        _ => None,
    }
}

/// Pull `"group:name"` out of a `spec.owner` field. `user:...` and
/// bare strings (no prefix) are treated as unresolved and return None —
/// the import handler falls back to `owner_group_id = NULL`.
fn parse_owner(owner: Option<&str>) -> Option<String> {
    let raw = owner?.trim();
    let rest = raw.strip_prefix("group:")?.trim();
    if rest.is_empty() {
        None
    } else {
        Some(rest.to_string())
    }
}

/// Split a `kind:name` reference like `resource:orders-rds` or a bare
/// `orders-rds` into `(target_kind, target_name)`. Unknown prefixes
/// (e.g. `user:alice`) are returned as-is with that prefix as the kind
/// so the handler can ignore non-catalog references upstream. Bare
/// strings return `(None, raw)`.
fn split_ref(raw: &str) -> Option<(Option<String>, String)> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    if let Some((prefix, rest)) = raw.split_once(':') {
        let name = rest.trim();
        if name.is_empty() {
            return None;
        }
        // Normalise to the DB form (`component`, `api`, …). Unknown prefixes
        // pass through so the handler can reject or log; `user:` is the
        // common case for non-catalog references today.
        let kind = match prefix.trim() {
            "System" | "system" => Some("system".to_string()),
            "Component" | "component" => Some("component".to_string()),
            "API" | "api" => Some("api".to_string()),
            "Resource" | "resource" => Some("resource".to_string()),
            "Group" | "group" => Some("group".to_string()),
            other => Some(other.to_string()),
        };
        Some((kind, name.to_string()))
    } else {
        Some((None, raw.to_string()))
    }
}

/// Extract the DeclaredRelation list from an entity's `spec` block.
/// Handles the five relation sources from design §4.2:
/// - `spec.system` → `part_of`
/// - `spec.owner: group:…` → `owns` (reverse direction: owner → entity)
/// - `spec.dependsOn: [ref, …]` → `depends_on`
/// - `spec.providesApis: [ref, …]` → `provides`
/// - `spec.consumesApis: [ref, …]` → `consumes`
///
/// `user:` prefixed owners are currently dropped — the handler falls
/// back to `owner_group_id = NULL` and logs nothing (we don't have a
/// user_ref column yet).
fn extract_declared_relations(spec: &serde_json::Value) -> Vec<DeclaredRelation> {
    let mut out = Vec::new();

    // spec.system → part_of (entity is_part_of system)
    if let Some(sys) = spec.get("system").and_then(|v| v.as_str())
        && let Some((_, name)) = split_ref(sys)
    {
        out.push(DeclaredRelation {
            target_name: name,
            target_kind: Some("system".to_string()),
            relation_type: RELATION_PART_OF.to_string(),
            direction: RelationDirection::FromEntity,
        });
    }

    // spec.owner: group:foo → owns (reverse: foo owns entity)
    // `user:` and bare strings are ignored; the `parse_owner` helper
    // already handles the positive group case.
    if let Some(owner_raw) = spec.get("owner").and_then(|v| v.as_str())
        && let Some(group_name) = parse_owner(Some(owner_raw))
    {
        out.push(DeclaredRelation {
            target_name: group_name,
            target_kind: Some("group".to_string()),
            relation_type: RELATION_OWNS.to_string(),
            direction: RelationDirection::ToEntity,
        });
    }

    // spec.dependsOn: [ref, …] → depends_on
    if let Some(arr) = spec.get("dependsOn").and_then(|v| v.as_array()) {
        for item in arr {
            if let Some(raw) = item.as_str()
                && let Some((kind, name)) = split_ref(raw)
            {
                out.push(DeclaredRelation {
                    target_name: name,
                    target_kind: kind,
                    relation_type: RELATION_DEPENDS_ON.to_string(),
                    direction: RelationDirection::FromEntity,
                });
            }
        }
    }

    // spec.providesApis: [ref, …] → provides
    if let Some(arr) = spec.get("providesApis").and_then(|v| v.as_array()) {
        for item in arr {
            if let Some(raw) = item.as_str()
                && let Some((kind, name)) = split_ref(raw)
            {
                out.push(DeclaredRelation {
                    target_name: name,
                    // Default to `api` when no prefix given — the
                    // providesApis list is API-specific by contract.
                    target_kind: kind.or_else(|| Some("api".to_string())),
                    relation_type: RELATION_PROVIDES.to_string(),
                    direction: RelationDirection::FromEntity,
                });
            }
        }
    }

    // spec.consumesApis: [ref, …] → consumes
    if let Some(arr) = spec.get("consumesApis").and_then(|v| v.as_array()) {
        for item in arr {
            if let Some(raw) = item.as_str()
                && let Some((kind, name)) = split_ref(raw)
            {
                out.push(DeclaredRelation {
                    target_name: name,
                    target_kind: kind.or_else(|| Some("api".to_string())),
                    relation_type: RELATION_CONSUMES.to_string(),
                    direction: RelationDirection::FromEntity,
                });
            }
        }
    }

    out
}

/// Strip out the well-known fields that land in typed columns / the
/// relations table so that `spec_remaining` only carries the
/// open-ended, entity-specific blob.
fn strip_well_known_spec(mut spec: serde_json::Value) -> serde_json::Value {
    if let Some(obj) = spec.as_object_mut() {
        obj.remove("lifecycle");
        obj.remove("owner");
        obj.remove("system");
        // Relation sources are represented in catalog_relations; leaving
        // them in spec would double-book the data between a JSONB copy
        // and the edge table.
        obj.remove("dependsOn");
        obj.remove("providesApis");
        obj.remove("consumesApis");
    }
    spec
}

/// Parse a multi-document YAML stream. Each document becomes one
/// `ParsedEntity`. Returns the first error encountered — partial
/// success is the handler's problem, not the parser's.
pub fn parse_multi_doc(yaml: &str) -> Result<Vec<ParsedEntity>, ParseError> {
    // P1 #11 safety gates — refuse clearly abusive payloads up front rather
    // than let serde_yaml run arbitrary alias expansion against untrusted
    // input.
    if yaml.len() > MAX_YAML_BYTES {
        return Err(ParseError::InvalidYaml("payload too large".into()));
    }
    let alias_markers = yaml.matches('*').count() + yaml.matches('&').count();
    if alias_markers > MAX_YAML_ALIAS_MARKERS {
        return Err(ParseError::InvalidYaml(
            "too many YAML aliases (possible bomb)".into(),
        ));
    }

    let mut out = Vec::new();

    for doc in serde_yaml::Deserializer::from_str(yaml) {
        let value = serde_yaml::Value::deserialize(doc)
            .map_err(|e| ParseError::InvalidYaml(e.to_string()))?;

        // Skip blank documents (empty `---\n---` separators).
        if value.is_null() {
            continue;
        }

        let doc: CatalogYamlDoc = serde_yaml::from_value(value)
            .map_err(|e| ParseError::InvalidYaml(e.to_string()))?;

        if doc.api_version != EXPECTED_API_VERSION {
            return Err(ParseError::InvalidApiVersion(doc.api_version));
        }

        let kind = normalise_kind(&doc.kind)
            .ok_or_else(|| ParseError::InvalidKind(doc.kind.clone()))?;

        let name = doc.metadata.name.trim().to_string();
        if name.is_empty() {
            return Err(ParseError::MissingName);
        }

        let lifecycle = doc
            .spec
            .get("lifecycle")
            .and_then(|v| v.as_str())
            .unwrap_or("experimental")
            .to_string();

        let owner_group_name =
            parse_owner(doc.spec.get("owner").and_then(|v| v.as_str()));

        let system_name = doc
            .spec
            .get("system")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.trim().to_string());

        let annotations = if doc.metadata.annotations.is_null() {
            serde_json::json!({})
        } else {
            doc.metadata.annotations
        };

        // Extract relation declarations before we strip the sources from
        // spec_remaining — after stripping the `spec` object no longer
        // carries dependsOn/providesApis/consumesApis/system/owner.
        let declared_relations = extract_declared_relations(&doc.spec);

        out.push(ParsedEntity {
            kind: kind.to_string(),
            name,
            display_name: doc.metadata.display_name,
            description: doc.metadata.description,
            lifecycle,
            owner_group_name,
            system_name,
            tags: doc.metadata.tags,
            annotations,
            spec_remaining: strip_well_known_spec(doc.spec),
            declared_relations,
        });

        if out.len() > MAX_YAML_DOCS {
            return Err(ParseError::InvalidYaml(
                "too many YAML documents in request".into(),
            ));
        }
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_single_component_doc() {
        let yaml = r#"
apiVersion: loops.yingchu.cloud/v1
kind: Component
metadata:
  name: order-api
  displayName: Order API
  description: Order REST service
  tags: [rust, critical]
  annotations:
    github.com/slug: acme/order-api
spec:
  lifecycle: production
  owner: group:platform-sre
  system: commerce
  type: service
"#;

        let parsed = parse_multi_doc(yaml).expect("parses");
        assert_eq!(parsed.len(), 1);
        let e = &parsed[0];
        assert_eq!(e.kind, "component");
        assert_eq!(e.name, "order-api");
        assert_eq!(e.display_name.as_deref(), Some("Order API"));
        assert_eq!(e.lifecycle, "production");
        assert_eq!(e.owner_group_name.as_deref(), Some("platform-sre"));
        assert_eq!(e.system_name.as_deref(), Some("commerce"));
        assert_eq!(e.tags, vec!["rust".to_string(), "critical".into()]);
        // lifecycle/owner/system should be stripped from spec_remaining.
        assert!(e.spec_remaining.get("lifecycle").is_none());
        assert!(e.spec_remaining.get("owner").is_none());
        assert!(e.spec_remaining.get("system").is_none());
        assert_eq!(
            e.spec_remaining.get("type").and_then(|v| v.as_str()),
            Some("service")
        );
    }

    #[test]
    fn parses_multi_doc_component_and_api() {
        let yaml = r#"
apiVersion: loops.yingchu.cloud/v1
kind: Component
metadata:
  name: billing
spec:
  lifecycle: production
  owner: group:billing-team
---
apiVersion: loops.yingchu.cloud/v1
kind: API
metadata:
  name: billing-api
spec:
  lifecycle: experimental
  owner: group:billing-team
"#;

        let parsed = parse_multi_doc(yaml).expect("parses");
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].kind, "component");
        assert_eq!(parsed[0].name, "billing");
        assert_eq!(parsed[1].kind, "api");
        assert_eq!(parsed[1].name, "billing-api");
        assert_eq!(
            parsed[1].owner_group_name.as_deref(),
            Some("billing-team")
        );
    }

    #[test]
    fn rejects_invalid_api_version() {
        let yaml = r#"
apiVersion: backstage.io/v1alpha1
kind: Component
metadata:
  name: legacy
"#;
        let err = parse_multi_doc(yaml).expect_err("rejects");
        assert!(matches!(err, ParseError::InvalidApiVersion(_)));
    }

    #[test]
    fn rejects_invalid_kind() {
        let yaml = r#"
apiVersion: loops.yingchu.cloud/v1
kind: Service
metadata:
  name: oops
"#;
        let err = parse_multi_doc(yaml).expect_err("rejects");
        assert!(matches!(err, ParseError::InvalidKind(_)));
    }

    #[test]
    fn rejects_missing_name() {
        let yaml = r#"
apiVersion: loops.yingchu.cloud/v1
kind: Component
metadata:
  name: "   "
spec:
  lifecycle: production
"#;
        let err = parse_multi_doc(yaml).expect_err("rejects");
        assert_eq!(err, ParseError::MissingName);
    }

    #[test]
    fn parse_owner_handles_all_three_forms() {
        assert_eq!(
            parse_owner(Some("group:platform-sre")),
            Some("platform-sre".to_string())
        );
        // user: prefix is currently ignored — only groups resolve.
        assert_eq!(parse_owner(Some("user:alice")), None);
        // bare string with no prefix is ignored.
        assert_eq!(parse_owner(Some("platform-sre")), None);
        // empty rest after prefix also ignored.
        assert_eq!(parse_owner(Some("group: ")), None);
        assert_eq!(parse_owner(None), None);
    }

    #[test]
    fn rejects_oversized_payload() {
        // 6 MB of whitespace — comfortably above the 5 MB gate but within
        // what serde_yaml would otherwise parse happily.
        let yaml = "# padding\n".repeat(6 * 1024 * 1024 / 10);
        let err = parse_multi_doc(&yaml).expect_err("rejects");
        assert!(matches!(err, ParseError::InvalidYaml(msg) if msg.contains("too large")));
    }

    #[test]
    fn rejects_too_many_aliases() {
        // 101 anchor markers — trips the YAML-bomb gate without requiring a
        // real parsable bomb (we want the gate itself covered).
        let mut yaml = String::from("apiVersion: loops.yingchu.cloud/v1\n");
        yaml.push_str(&"&".repeat(101));
        let err = parse_multi_doc(&yaml).expect_err("rejects");
        assert!(matches!(err, ParseError::InvalidYaml(msg) if msg.contains("alias")));
    }

    #[test]
    fn lifecycle_defaults_to_experimental_when_absent() {
        let yaml = r#"
apiVersion: loops.yingchu.cloud/v1
kind: Resource
metadata:
  name: my-bucket
spec:
  type: s3-bucket
"#;
        let parsed = parse_multi_doc(yaml).expect("parses");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].lifecycle, "experimental");
        assert_eq!(parsed[0].kind, "resource");
    }

    // ─── W1 design §4.2: declared_relations extraction ──────────────

    #[test]
    fn test_extracts_depends_on_relations() {
        // Both prefixed (`resource:foo`) and prefix-less references
        // should land in the depends_on bucket.
        let yaml = r#"
apiVersion: loops.yingchu.cloud/v1
kind: Component
metadata:
  name: order-api
spec:
  lifecycle: production
  dependsOn:
    - resource:orders-rds
    - component:auth-service
    - billing-api
"#;
        let parsed = parse_multi_doc(yaml).expect("parses");
        let rels = &parsed[0].declared_relations;
        let depends_on: Vec<&DeclaredRelation> = rels
            .iter()
            .filter(|r| r.relation_type == RELATION_DEPENDS_ON)
            .collect();
        assert_eq!(
            depends_on.len(),
            3,
            "expected 3 depends_on relations, got {:?}",
            depends_on
        );
        // Prefix preserved as target_kind
        assert!(
            depends_on
                .iter()
                .any(|r| r.target_name == "orders-rds"
                    && r.target_kind.as_deref() == Some("resource"))
        );
        assert!(
            depends_on
                .iter()
                .any(|r| r.target_name == "auth-service"
                    && r.target_kind.as_deref() == Some("component"))
        );
        // Bare name → target_kind None so handler can match any kind
        assert!(
            depends_on
                .iter()
                .any(|r| r.target_name == "billing-api" && r.target_kind.is_none())
        );
        // All are forward-direction edges (entity → target)
        for r in depends_on {
            assert_eq!(r.direction, RelationDirection::FromEntity);
        }
    }

    #[test]
    fn test_extracts_provides_consumes_apis() {
        let yaml = r#"
apiVersion: loops.yingchu.cloud/v1
kind: Component
metadata:
  name: order-api
spec:
  lifecycle: production
  providesApis:
    - order-api-v1
    - api:order-api-v2
  consumesApis:
    - user-api-v1
"#;
        let parsed = parse_multi_doc(yaml).expect("parses");
        let rels = &parsed[0].declared_relations;
        let provides: Vec<&DeclaredRelation> = rels
            .iter()
            .filter(|r| r.relation_type == RELATION_PROVIDES)
            .collect();
        let consumes: Vec<&DeclaredRelation> = rels
            .iter()
            .filter(|r| r.relation_type == RELATION_CONSUMES)
            .collect();

        assert_eq!(provides.len(), 2, "expected 2 provides relations");
        assert_eq!(consumes.len(), 1, "expected 1 consumes relation");

        // Prefix-less names default to target_kind=api because
        // providesApis / consumesApis are API-typed by contract.
        assert!(
            provides
                .iter()
                .any(|r| r.target_name == "order-api-v1"
                    && r.target_kind.as_deref() == Some("api"))
        );
        assert!(
            provides
                .iter()
                .any(|r| r.target_name == "order-api-v2"
                    && r.target_kind.as_deref() == Some("api"))
        );
        assert_eq!(consumes[0].target_name, "user-api-v1");
        assert_eq!(consumes[0].target_kind.as_deref(), Some("api"));
        assert_eq!(consumes[0].direction, RelationDirection::FromEntity);
    }

    #[test]
    fn test_extracts_system_relation() {
        let yaml = r#"
apiVersion: loops.yingchu.cloud/v1
kind: Component
metadata:
  name: order-api
spec:
  lifecycle: production
  system: payments
"#;
        let parsed = parse_multi_doc(yaml).expect("parses");
        let part_of: Vec<&DeclaredRelation> = parsed[0]
            .declared_relations
            .iter()
            .filter(|r| r.relation_type == RELATION_PART_OF)
            .collect();
        assert_eq!(part_of.len(), 1);
        assert_eq!(part_of[0].target_name, "payments");
        assert_eq!(part_of[0].target_kind.as_deref(), Some("system"));
        // entity is_part_of system → forward edge (entity, system)
        assert_eq!(part_of[0].direction, RelationDirection::FromEntity);
    }

    #[test]
    fn test_extracts_owner_relation_reverse() {
        // owner emits the only reverse-direction edge — the owner owns
        // the entity, so the relation row is (owner, entity, 'owns').
        let yaml = r#"
apiVersion: loops.yingchu.cloud/v1
kind: Component
metadata:
  name: order-api
spec:
  lifecycle: production
  owner: group:payments-team
"#;
        let parsed = parse_multi_doc(yaml).expect("parses");
        let owns: Vec<&DeclaredRelation> = parsed[0]
            .declared_relations
            .iter()
            .filter(|r| r.relation_type == RELATION_OWNS)
            .collect();
        assert_eq!(owns.len(), 1);
        assert_eq!(owns[0].target_name, "payments-team");
        assert_eq!(owns[0].target_kind.as_deref(), Some("group"));
        // Reverse direction — current entity is the to_id.
        assert_eq!(owns[0].direction, RelationDirection::ToEntity);

        // user: prefix is currently dropped (no user_ref column).
        let yaml_user = r#"
apiVersion: loops.yingchu.cloud/v1
kind: Component
metadata:
  name: order-api
spec:
  lifecycle: production
  owner: user:alice
"#;
        let parsed_user = parse_multi_doc(yaml_user).expect("parses");
        assert!(
            parsed_user[0]
                .declared_relations
                .iter()
                .all(|r| r.relation_type != RELATION_OWNS)
        );
    }

    #[test]
    fn test_prefix_syntax_parses_same_target_for_both_forms() {
        // Same underlying target_name whether the YAML uses a prefix or
        // the bare form; target_kind differs so the handler can disambiguate.
        let (kind_a, name_a) =
            split_ref("resource:orders-rds").expect("kind:name parses");
        let (kind_b, name_b) = split_ref("orders-rds").expect("bare parses");
        assert_eq!(name_a, "orders-rds");
        assert_eq!(name_b, "orders-rds");
        assert_eq!(kind_a.as_deref(), Some("resource"));
        assert!(kind_b.is_none());

        // Casing of the prefix is normalised to the DB form.
        let (kind_c, name_c) = split_ref("Component:auth-service").expect("Component: parses");
        assert_eq!(kind_c.as_deref(), Some("component"));
        assert_eq!(name_c, "auth-service");

        // Empty / whitespace-only references are rejected.
        assert!(split_ref("").is_none());
        assert!(split_ref("resource:").is_none());
        assert!(split_ref("   ").is_none());
    }

    #[test]
    fn test_strip_well_known_spec_removes_relation_sources() {
        // The relation source fields must not survive into spec_remaining
        // or catalog_entities.spec would double-book the data.
        let yaml = r#"
apiVersion: loops.yingchu.cloud/v1
kind: Component
metadata:
  name: order-api
spec:
  lifecycle: production
  owner: group:payments-team
  system: payments
  dependsOn: [resource:db]
  providesApis: [order-api-v1]
  consumesApis: [user-api-v1]
  type: service
"#;
        let parsed = parse_multi_doc(yaml).expect("parses");
        let sr = &parsed[0].spec_remaining;
        assert!(sr.get("system").is_none());
        assert!(sr.get("owner").is_none());
        assert!(sr.get("dependsOn").is_none());
        assert!(sr.get("providesApis").is_none());
        assert!(sr.get("consumesApis").is_none());
        // type is preserved — it's a kind-specific field, not a relation source.
        assert_eq!(sr.get("type").and_then(|v| v.as_str()), Some("service"));
    }
}
