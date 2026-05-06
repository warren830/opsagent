-- Phase 1 scaffold: add `source` column to `catalog_relations` so the
-- k8s_inferer background job can distinguish its inferred edges from
-- relations that came from YAML import. Future sources (trace-driven,
-- eBPF, etc.) plug in here without schema churn.
--
-- Values we emit today:
--   'declared'     - written by yaml_parser when the catalog-info.yaml
--                    file lists an explicit relations: block.
--   'k8s_selector' - written by services::catalog::k8s_inferer when a
--                    K8s Service.spec.selector matches a workload we
--                    already have a Component for.
--
-- Existing rows default to 'declared' (the only source that ever wrote
-- to this table before today). Deletions from the inferer loop MUST be
-- scoped by `source` so we never touch declared edges.

ALTER TABLE catalog_relations
    ADD COLUMN IF NOT EXISTS source VARCHAR(32) NOT NULL DEFAULT 'declared';

CREATE INDEX IF NOT EXISTS idx_catalog_relations_source
    ON catalog_relations(source);
