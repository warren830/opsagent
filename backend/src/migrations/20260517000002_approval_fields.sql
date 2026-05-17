-- Add new fields to approvals table for Jira integration, plan details, mark result, and withdraw
ALTER TABLE approvals ADD COLUMN IF NOT EXISTS jira_key TEXT;
ALTER TABLE approvals ADD COLUMN IF NOT EXISTS plan_detail JSONB;
ALTER TABLE approvals ADD COLUMN IF NOT EXISTS marked_by UUID REFERENCES users(id) ON DELETE SET NULL;
ALTER TABLE approvals ADD COLUMN IF NOT EXISTS withdrawn_at TIMESTAMPTZ;
