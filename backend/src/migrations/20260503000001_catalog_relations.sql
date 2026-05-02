-- W1 Catalog: typed relations, group membership, and import audit trail.
-- See docs/platform-evolution.md §三 and .claude/plans/plan-tingly-cherny.md W1.

CREATE TABLE IF NOT EXISTS catalog_relations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    from_id UUID NOT NULL REFERENCES catalog_entities(id) ON DELETE CASCADE,
    to_id UUID NOT NULL REFERENCES catalog_entities(id) ON DELETE CASCADE,
    relation_type VARCHAR(32) NOT NULL
        CHECK (relation_type IN ('owns','provides','consumes','depends_on','part_of','deployed_on')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (from_id, to_id, relation_type)
);
CREATE INDEX IF NOT EXISTS idx_catalog_relations_from ON catalog_relations(from_id, relation_type);
CREATE INDEX IF NOT EXISTS idx_catalog_relations_to ON catalog_relations(to_id, relation_type);

CREATE TABLE IF NOT EXISTS catalog_group_members (
    group_id UUID NOT NULL REFERENCES catalog_entities(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role VARCHAR(32) NOT NULL DEFAULT 'member'
        CHECK (role IN ('owner','member')),
    PRIMARY KEY (group_id, user_id)
);
CREATE INDEX IF NOT EXISTS idx_catalog_group_members_user ON catalog_group_members(user_id);

CREATE TABLE IF NOT EXISTS catalog_import_runs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    source VARCHAR(32) NOT NULL,
    source_ref TEXT,
    entities_created INT NOT NULL DEFAULT 0,
    entities_updated INT NOT NULL DEFAULT 0,
    errors JSONB NOT NULL DEFAULT '[]',
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMPTZ
);
