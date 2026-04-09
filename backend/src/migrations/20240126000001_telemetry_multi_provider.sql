-- Support multiple telemetry configs per tenant (multi-provider routing).
-- Each config gets a name + routing rules (which signals to send where).

-- Add new columns
ALTER TABLE telemetry_config ADD COLUMN IF NOT EXISTS name VARCHAR(100);
ALTER TABLE telemetry_config ADD COLUMN IF NOT EXISTS routing JSONB NOT NULL DEFAULT '{"signals":["metrics","logs","traces"],"scope":"all"}';

-- Backfill name from provider for existing rows
UPDATE telemetry_config SET name = provider WHERE name IS NULL;
ALTER TABLE telemetry_config ALTER COLUMN name SET NOT NULL;

-- Deduplicate: keep the newest row per (tenant_id, name), delete older duplicates
DELETE FROM telemetry_config a USING telemetry_config b
WHERE LOWER(a.name) = LOWER(b.name)
  AND a.tenant_id IS NOT DISTINCT FROM b.tenant_id
  AND a.created_at < b.created_at;

-- Drop old unique constraint (one config per tenant) → new (name unique per tenant)
DROP INDEX IF EXISTS idx_telemetry_config_tenant;
CREATE UNIQUE INDEX idx_telemetry_config_tenant_name
  ON telemetry_config(COALESCE(tenant_id, '00000000-0000-0000-0000-000000000000'::uuid), LOWER(name));
