-- Fix: ON CONFLICT (tenant_id, name) fails when tenant_id IS NULL
-- because PostgreSQL treats NULL != NULL in unique indexes.

-- Step 1: Clean up duplicate discovered clusters FIRST (keep the latest per name+tenant)
DELETE FROM clusters a
  USING clusters b
  WHERE a.name = b.name
    AND a.tenant_id IS NOT DISTINCT FROM b.tenant_id
    AND a.is_discovered = true
    AND b.is_discovered = true
    AND a.created_at < b.created_at;

-- Step 2: Drop old indexes
DROP INDEX IF EXISTS idx_clusters_tenant_name;
DROP INDEX IF EXISTS idx_cloud_accounts_tenant_account_org;

-- Step 3: Create functional unique indexes using COALESCE (NULL-safe)
CREATE UNIQUE INDEX idx_clusters_tenant_name
  ON clusters (COALESCE(tenant_id, '00000000-0000-0000-0000-000000000000'::uuid), name);

CREATE UNIQUE INDEX idx_cloud_accounts_tenant_account_org
  ON cloud_accounts (COALESCE(tenant_id, '00000000-0000-0000-0000-000000000000'::uuid), account_id)
  WHERE source = 'organization';
