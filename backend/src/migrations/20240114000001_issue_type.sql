-- Add issue_type to distinguish incidents (alert-triggered) from predictions (scheduler-detected)
ALTER TABLE issues ADD COLUMN IF NOT EXISTS issue_type VARCHAR(20) NOT NULL DEFAULT 'incident';
CREATE INDEX IF NOT EXISTS idx_issues_type ON issues(issue_type);
