-- Add user_id + visibility to glossary, knowledge_files, mcp_servers, scheduled_jobs
-- Same pattern as skills: private (user_id=X) vs public (user_id=NULL, tenant-wide)

-- Glossary
ALTER TABLE glossary ADD COLUMN IF NOT EXISTS user_id UUID REFERENCES users(id) ON DELETE CASCADE;
ALTER TABLE glossary ADD COLUMN IF NOT EXISTS visibility VARCHAR(20) NOT NULL DEFAULT 'public';
CREATE INDEX IF NOT EXISTS idx_glossary_user ON glossary(user_id);
CREATE INDEX IF NOT EXISTS idx_glossary_visibility ON glossary(visibility);

-- Knowledge files
ALTER TABLE knowledge_files ADD COLUMN IF NOT EXISTS user_id UUID REFERENCES users(id) ON DELETE CASCADE;
ALTER TABLE knowledge_files ADD COLUMN IF NOT EXISTS visibility VARCHAR(20) NOT NULL DEFAULT 'public';
CREATE INDEX IF NOT EXISTS idx_knowledge_user ON knowledge_files(user_id);
CREATE INDEX IF NOT EXISTS idx_knowledge_visibility ON knowledge_files(visibility);

-- MCP servers
ALTER TABLE mcp_servers ADD COLUMN IF NOT EXISTS user_id UUID REFERENCES users(id) ON DELETE CASCADE;
ALTER TABLE mcp_servers ADD COLUMN IF NOT EXISTS visibility VARCHAR(20) NOT NULL DEFAULT 'public';
ALTER TABLE mcp_servers ADD COLUMN IF NOT EXISTS created_by UUID REFERENCES users(id) ON DELETE SET NULL;
CREATE INDEX IF NOT EXISTS idx_mcp_user ON mcp_servers(user_id);
CREATE INDEX IF NOT EXISTS idx_mcp_visibility ON mcp_servers(visibility);

-- Scheduled jobs
ALTER TABLE scheduled_jobs ADD COLUMN IF NOT EXISTS user_id UUID REFERENCES users(id) ON DELETE CASCADE;
ALTER TABLE scheduled_jobs ADD COLUMN IF NOT EXISTS visibility VARCHAR(20) NOT NULL DEFAULT 'public';
ALTER TABLE scheduled_jobs ADD COLUMN IF NOT EXISTS created_by UUID REFERENCES users(id) ON DELETE SET NULL;
CREATE INDEX IF NOT EXISTS idx_scheduled_jobs_user ON scheduled_jobs(user_id);
CREATE INDEX IF NOT EXISTS idx_scheduled_jobs_visibility ON scheduled_jobs(visibility);
