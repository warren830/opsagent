-- Batch 3: Resources table

CREATE TABLE IF NOT EXISTS resources (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    resource_type VARCHAR(50) NOT NULL,
    name VARCHAR(200) NOT NULL,
    account_id VARCHAR(100),
    region VARCHAR(50),
    arn VARCHAR(500),
    status VARCHAR(30) NOT NULL DEFAULT 'active',
    tags JSONB NOT NULL DEFAULT '{}',
    raw_data JSONB NOT NULL DEFAULT '{}',
    tenant_id UUID REFERENCES tenants(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_resources_tenant ON resources(tenant_id);
CREATE INDEX IF NOT EXISTS idx_resources_type ON resources(resource_type);
CREATE INDEX IF NOT EXISTS idx_resources_region ON resources(region);
