-- Add missing unique constraints to prevent duplicate data
-- Pattern: COALESCE(tenant_id, UUID_ZERO) for NULL-safe multi-tenant uniqueness

-- ─── cloud_accounts: prevent duplicate manual accounts ──────────────────────
-- org-sync already has idx_cloud_accounts_tenant_account_org
-- Manual accounts need: same tenant + same account_id can't be added twice
-- First clean up any existing duplicates (keep newest)
DELETE FROM cloud_accounts a USING cloud_accounts b
WHERE a.account_id = b.account_id
  AND a.tenant_id IS NOT DISTINCT FROM b.tenant_id
  AND a.source = b.source
  AND a.created_at < b.created_at;

CREATE UNIQUE INDEX IF NOT EXISTS idx_cloud_accounts_tenant_account_manual
ON cloud_accounts (COALESCE(tenant_id, '00000000-0000-0000-0000-000000000000'::uuid), account_id)
WHERE source = 'manual';

-- ─── skills: unique name per visibility scope ───────────────────────────────
-- public skills: unique per tenant
-- private skills: unique per user
DELETE FROM skills a USING skills b
WHERE LOWER(a.name) = LOWER(b.name)
  AND a.tenant_id IS NOT DISTINCT FROM b.tenant_id
  AND a.visibility = b.visibility
  AND COALESCE(a.user_id, '00000000-0000-0000-0000-000000000000'::uuid) = COALESCE(b.user_id, '00000000-0000-0000-0000-000000000000'::uuid)
  AND a.created_at < b.created_at;

CREATE UNIQUE INDEX IF NOT EXISTS idx_skills_tenant_name_public
ON skills (COALESCE(tenant_id, '00000000-0000-0000-0000-000000000000'::uuid), LOWER(name))
WHERE visibility = 'public';

CREATE UNIQUE INDEX IF NOT EXISTS idx_skills_user_name_private
ON skills (user_id, LOWER(name))
WHERE visibility = 'private';

-- ─── mcp_servers: unique name per visibility scope ──────────────────────────
DELETE FROM mcp_servers a USING mcp_servers b
WHERE LOWER(a.name) = LOWER(b.name)
  AND a.tenant_id IS NOT DISTINCT FROM b.tenant_id
  AND a.visibility = b.visibility
  AND COALESCE(a.user_id, '00000000-0000-0000-0000-000000000000'::uuid) = COALESCE(b.user_id, '00000000-0000-0000-0000-000000000000'::uuid)
  AND a.created_at < b.created_at;

CREATE UNIQUE INDEX IF NOT EXISTS idx_mcp_servers_tenant_name_public
ON mcp_servers (COALESCE(tenant_id, '00000000-0000-0000-0000-000000000000'::uuid), LOWER(name))
WHERE visibility = 'public';

CREATE UNIQUE INDEX IF NOT EXISTS idx_mcp_servers_user_name_private
ON mcp_servers (user_id, LOWER(name))
WHERE visibility = 'private';

-- ─── channels: unique name per tenant ───────────────────────────────────────
DELETE FROM channels a USING channels b
WHERE LOWER(a.name) = LOWER(b.name)
  AND a.tenant_id IS NOT DISTINCT FROM b.tenant_id
  AND a.created_at < b.created_at;

CREATE UNIQUE INDEX IF NOT EXISTS idx_channels_tenant_name
ON channels (COALESCE(tenant_id, '00000000-0000-0000-0000-000000000000'::uuid), LOWER(name));

-- ─── scheduled_jobs: unique name per visibility scope ───────────────────────
DELETE FROM scheduled_jobs a USING scheduled_jobs b
WHERE LOWER(a.name) = LOWER(b.name)
  AND a.tenant_id IS NOT DISTINCT FROM b.tenant_id
  AND a.visibility = b.visibility
  AND COALESCE(a.user_id, '00000000-0000-0000-0000-000000000000'::uuid) = COALESCE(b.user_id, '00000000-0000-0000-0000-000000000000'::uuid)
  AND a.created_at < b.created_at;

CREATE UNIQUE INDEX IF NOT EXISTS idx_scheduled_jobs_tenant_name_public
ON scheduled_jobs (COALESCE(tenant_id, '00000000-0000-0000-0000-000000000000'::uuid), LOWER(name))
WHERE visibility = 'public';

CREATE UNIQUE INDEX IF NOT EXISTS idx_scheduled_jobs_user_name_private
ON scheduled_jobs (user_id, LOWER(name))
WHERE visibility = 'private';

-- ─── knowledge_files: unique filename per visibility scope ──────────────────
DELETE FROM knowledge_files a USING knowledge_files b
WHERE LOWER(a.filename) = LOWER(b.filename)
  AND a.tenant_id IS NOT DISTINCT FROM b.tenant_id
  AND a.visibility = b.visibility
  AND COALESCE(a.user_id, '00000000-0000-0000-0000-000000000000'::uuid) = COALESCE(b.user_id, '00000000-0000-0000-0000-000000000000'::uuid)
  AND a.created_at < b.created_at;

CREATE UNIQUE INDEX IF NOT EXISTS idx_knowledge_files_tenant_name_public
ON knowledge_files (COALESCE(tenant_id, '00000000-0000-0000-0000-000000000000'::uuid), LOWER(filename))
WHERE visibility = 'public';

CREATE UNIQUE INDEX IF NOT EXISTS idx_knowledge_files_user_name_private
ON knowledge_files (user_id, LOWER(filename))
WHERE visibility = 'private';

-- ─── pipeline_repos: unique repo per tenant ─────────────────────────────────
DELETE FROM pipeline_repos a USING pipeline_repos b
WHERE a.repository = b.repository
  AND a.tenant_id IS NOT DISTINCT FROM b.tenant_id
  AND a.created_at < b.created_at;

CREATE UNIQUE INDEX IF NOT EXISTS idx_pipeline_repos_tenant_repo
ON pipeline_repos (COALESCE(tenant_id, '00000000-0000-0000-0000-000000000000'::uuid), repository);

-- ─── resources: unique ARN per tenant ───────────────────────────────────────
DELETE FROM resources a USING resources b
WHERE a.arn = b.arn
  AND a.arn IS NOT NULL
  AND a.tenant_id IS NOT DISTINCT FROM b.tenant_id
  AND a.created_at < b.created_at;

CREATE UNIQUE INDEX IF NOT EXISTS idx_resources_tenant_arn
ON resources (COALESCE(tenant_id, '00000000-0000-0000-0000-000000000000'::uuid), arn)
WHERE arn IS NOT NULL;

-- ─── glossary: global uniqueness on term (case-insensitive) ─────────────────
-- Replace the split constraints with a single global one
-- First clean up duplicates
DELETE FROM glossary a USING glossary b
WHERE LOWER(a.term) = LOWER(b.term)
  AND a.id != b.id
  AND a.created_at < b.created_at;

DROP INDEX IF EXISTS idx_glossary_term_account;
DROP INDEX IF EXISTS idx_glossary_term_global;

CREATE UNIQUE INDEX idx_glossary_term_tenant
ON glossary (COALESCE(tenant_id, '00000000-0000-0000-0000-000000000000'::uuid), LOWER(term));
