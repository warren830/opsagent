-- Skills v2: user-level ownership, git-based installation, visibility control

ALTER TABLE skills ADD COLUMN IF NOT EXISTS user_id UUID REFERENCES users(id) ON DELETE CASCADE;
ALTER TABLE skills ADD COLUMN IF NOT EXISTS git_url TEXT;
ALTER TABLE skills ADD COLUMN IF NOT EXISTS repo_path TEXT;
ALTER TABLE skills ADD COLUMN IF NOT EXISTS visibility VARCHAR(20) NOT NULL DEFAULT 'private';

CREATE INDEX IF NOT EXISTS idx_skills_user ON skills(user_id);
CREATE INDEX IF NOT EXISTS idx_skills_visibility ON skills(visibility);
