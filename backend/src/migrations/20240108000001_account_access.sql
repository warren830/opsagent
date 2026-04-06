-- User-Account access control: many-to-many authorization table
CREATE TABLE IF NOT EXISTS user_account_access (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  account_id UUID NOT NULL REFERENCES cloud_accounts(id) ON DELETE CASCADE,
  role VARCHAR(20) NOT NULL DEFAULT 'readonly',  -- 'admin' | 'readonly'
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  UNIQUE(user_id, account_id)
);

-- Add account_id to glossary and knowledge for account-level binding
ALTER TABLE glossary ADD COLUMN IF NOT EXISTS account_id UUID REFERENCES cloud_accounts(id) ON DELETE SET NULL;
ALTER TABLE knowledge_files ADD COLUMN IF NOT EXISTS account_id UUID REFERENCES cloud_accounts(id) ON DELETE SET NULL;

-- Clean slate: old data was tenant-bound, new model is account-bound
TRUNCATE glossary;
TRUNCATE knowledge_files;
