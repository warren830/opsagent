CREATE TABLE IF NOT EXISTS channel_tenant_assignments (
    channel_id  UUID NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    tenant_id   UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (channel_id, tenant_id)
);

CREATE INDEX IF NOT EXISTS idx_cta_tenant ON channel_tenant_assignments (tenant_id);

-- Backfill: migrate existing channels' tenant_id into the new join table
INSERT INTO channel_tenant_assignments (channel_id, tenant_id)
SELECT id, tenant_id FROM channels WHERE tenant_id IS NOT NULL
ON CONFLICT DO NOTHING;
