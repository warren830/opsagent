-- Provider → Tenant assignment table.
-- Allows super_admin to assign shared (tenant_id IS NULL) providers to specific tenants,
-- and track which provider is the "default" for that tenant.
CREATE TABLE IF NOT EXISTS provider_tenant_assignments (
    provider_id UUID NOT NULL REFERENCES providers(id) ON DELETE CASCADE,
    tenant_id   UUID NOT NULL REFERENCES tenants(id)   ON DELETE CASCADE,
    is_default  BOOLEAN NOT NULL DEFAULT false,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (provider_id, tenant_id)
);

CREATE INDEX IF NOT EXISTS idx_pta_tenant ON provider_tenant_assignments (tenant_id);
