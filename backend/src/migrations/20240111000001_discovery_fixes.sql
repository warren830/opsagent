-- Fix: org discover ON CONFLICT needs a partial unique index
CREATE UNIQUE INDEX IF NOT EXISTS idx_cloud_accounts_tenant_account_org
  ON cloud_accounts (tenant_id, account_id)
  WHERE source = 'organization';

-- EKS discover upsert needs a unique constraint
CREATE UNIQUE INDEX IF NOT EXISTS idx_clusters_tenant_name
  ON clusters (tenant_id, name);
