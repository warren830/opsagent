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

use serde::Deserialize;

const EXPECTED_API_VERSION: &str = "loops.yingchu.cloud/v1";

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

/// Strip out the well-known fields that land in typed columns so that
/// `spec_remaining` only carries the open-ended, entity-specific blob.
fn strip_well_known_spec(mut spec: serde_json::Value) -> serde_json::Value {
    if let Some(obj) = spec.as_object_mut() {
        obj.remove("lifecycle");
        obj.remove("owner");
        obj.remove("system");
    }
    spec
}

/// Parse a multi-document YAML stream. Each document becomes one
/// `ParsedEntity`. Returns the first error encountered — partial
/// success is the handler's problem, not the parser's.
pub fn parse_multi_doc(yaml: &str) -> Result<Vec<ParsedEntity>, ParseError> {
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
        });
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
}
