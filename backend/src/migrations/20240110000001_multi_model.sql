-- Multi-model support: add name column and allow multiple providers per tenant
ALTER TABLE providers ADD COLUMN IF NOT EXISTS name VARCHAR(100) NOT NULL DEFAULT 'Default';

-- Unique: same name per tenant (partial indexes for NULL tenant_id)
CREATE UNIQUE INDEX IF NOT EXISTS idx_providers_tenant_name ON providers (tenant_id, name) WHERE tenant_id IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS idx_providers_global_name ON providers (name) WHERE tenant_id IS NULL;
