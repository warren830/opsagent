-- Add source tracking for Jira/Confluence knowledge sync
ALTER TABLE knowledge_files ADD COLUMN IF NOT EXISTS source VARCHAR(20) NOT NULL DEFAULT 'manual';
ALTER TABLE knowledge_files ADD COLUMN IF NOT EXISTS source_id VARCHAR(255);
ALTER TABLE knowledge_files ADD COLUMN IF NOT EXISTS source_url TEXT;
ALTER TABLE knowledge_files ADD COLUMN IF NOT EXISTS source_updated_at TIMESTAMPTZ;

-- Unique constraint: one knowledge file per external source item
CREATE UNIQUE INDEX IF NOT EXISTS idx_knowledge_source
ON knowledge_files(source, source_id)
WHERE source_id IS NOT NULL;

-- Index for filtering by source
CREATE INDEX IF NOT EXISTS idx_knowledge_source_type ON knowledge_files(source);
