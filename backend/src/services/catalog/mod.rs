//! Catalog service layer — YAML import + K8s auto-discovery.
//!
//! `yaml_parser` converts Backstage-style `catalog-info.yaml` documents
//! into `ParsedEntity` records that the HTTP handler upserts into
//! `catalog_entities`. `k8s_discovery` scans a cluster for workloads and
//! returns `DiscoveredComponent` candidates grouped by
//! `app.kubernetes.io/name`.

pub mod k8s_discovery;
pub mod yaml_parser;
