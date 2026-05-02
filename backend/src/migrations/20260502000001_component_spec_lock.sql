-- W0 spec-lock for platform evolution (Catalog/SLO/Incident parallel dev).
-- Minimum viable Component table + cross-module FK columns.
-- Full Catalog features land in feat/catalog-mvp.
-- See docs/platform-evolution.md §三 and .claude/plans/plan-tingly-cherny.md W0.

CREATE TABLE IF NOT EXISTS catalog_entities (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    kind VARCHAR(32) NOT NULL
        CHECK (kind IN ('system','component','api','resource','group')),
    name VARCHAR(128) NOT NULL,
    display_name VARCHAR(256),
    description TEXT,
    lifecycle VARCHAR(32) NOT NULL DEFAULT 'experimental'
        CHECK (lifecycle IN ('production','experimental','deprecated','retired')),
    owner_group_id UUID REFERENCES catalog_entities(id) ON DELETE SET NULL,
    system_id UUID REFERENCES catalog_entities(id) ON DELETE SET NULL,
    tags TEXT[] NOT NULL DEFAULT '{}',
    annotations JSONB NOT NULL DEFAULT '{}',
    source_url TEXT,
    source_ref VARCHAR(128),
    spec JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (tenant_id, kind, name)
);
CREATE INDEX IF NOT EXISTS idx_catalog_entities_tenant_kind ON catalog_entities(tenant_id, kind);
CREATE INDEX IF NOT EXISTS idx_catalog_entities_owner ON catalog_entities(owner_group_id);
CREATE INDEX IF NOT EXISTS idx_catalog_entities_system ON catalog_entities(system_id);

-- Cross-module FKs that SLO / Incident modules will depend on.
ALTER TABLE issues
  ADD COLUMN IF NOT EXISTS affected_component_ids UUID[] NOT NULL DEFAULT '{}',
  ADD COLUMN IF NOT EXISTS slo_id UUID;
CREATE INDEX IF NOT EXISTS idx_issues_components ON issues USING GIN (affected_component_ids);

-- Agent session context routing (Decision #4 from platform-evolution.md §6.1).
ALTER TABLE claude_sessions
  ADD COLUMN IF NOT EXISTS context_type VARCHAR(32),
  ADD COLUMN IF NOT EXISTS context_id UUID;

-- Link deployment history to Component for timeline aggregation.
ALTER TABLE deployment_events
  ADD COLUMN IF NOT EXISTS component_id UUID REFERENCES catalog_entities(id) ON DELETE SET NULL;
CREATE INDEX IF NOT EXISTS idx_deployment_events_component ON deployment_events(component_id);
