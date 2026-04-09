-- Remove unused visibility/user_id from glossary and knowledge_files.
-- These tables use account-based access control only — no public/private distinction.

-- ─── Glossary: drop visibility-based unique indexes and columns ────────────
DROP INDEX IF EXISTS idx_glossary_user;
DROP INDEX IF EXISTS idx_glossary_visibility;

ALTER TABLE glossary DROP COLUMN IF EXISTS user_id;
ALTER TABLE glossary DROP COLUMN IF EXISTS visibility;

-- ─── Knowledge files: replace visibility-based unique indexes ──────────────
DROP INDEX IF EXISTS idx_knowledge_files_tenant_name_public;
DROP INDEX IF EXISTS idx_knowledge_files_user_name_private;
DROP INDEX IF EXISTS idx_knowledge_user;
DROP INDEX IF EXISTS idx_knowledge_visibility;

ALTER TABLE knowledge_files DROP COLUMN IF EXISTS user_id;
ALTER TABLE knowledge_files DROP COLUMN IF EXISTS visibility;

-- Add account-based unique index for knowledge_files (filename unique per account)
DELETE FROM knowledge_files a USING knowledge_files b
WHERE LOWER(a.filename) = LOWER(b.filename)
  AND a.account_id IS NOT DISTINCT FROM b.account_id
  AND a.created_at < b.created_at;

CREATE UNIQUE INDEX IF NOT EXISTS idx_knowledge_files_account_filename
ON knowledge_files (COALESCE(account_id, '00000000-0000-0000-0000-000000000000'::uuid), LOWER(filename));
