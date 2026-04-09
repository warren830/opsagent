-- Extend scheduled_jobs with job type and skill support
ALTER TABLE scheduled_jobs ADD COLUMN IF NOT EXISTS job_type VARCHAR(20) NOT NULL DEFAULT 'agent';
-- job_type: 'builtin' (Rust native), 'agent' (free query), 'skill' (skill + params)

ALTER TABLE scheduled_jobs ADD COLUMN IF NOT EXISTS skill_path TEXT;
-- e.g. "~/.claude/skills/aws-news-watch/instructions.md"

ALTER TABLE scheduled_jobs ADD COLUMN IF NOT EXISTS skill_params JSONB NOT NULL DEFAULT '{}';
-- pre-filled parameters for headless skill execution

-- Make query optional (builtin tasks don't need it)
ALTER TABLE scheduled_jobs ALTER COLUMN query DROP NOT NULL;
ALTER TABLE scheduled_jobs ALTER COLUMN query SET DEFAULT '';

-- Job execution history
CREATE TABLE IF NOT EXISTS job_runs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    job_id UUID NOT NULL REFERENCES scheduled_jobs(id) ON DELETE CASCADE,
    status VARCHAR(20) NOT NULL DEFAULT 'pending',
    -- status: pending, running, success, failed
    trigger VARCHAR(20) NOT NULL DEFAULT 'scheduled',
    -- trigger: scheduled, manual
    started_at TIMESTAMPTZ,
    finished_at TIMESTAMPTZ,
    duration_ms BIGINT,
    summary TEXT,
    -- one-line summary for list view
    output TEXT,
    -- full Agent/Skill stdout (markdown)
    result JSONB,
    -- structured result for builtin tasks, e.g. {"added":2,"removed":1}
    error TEXT,
    -- error message if failed
    exit_code INT,
    -- subprocess exit code (for agent/skill tasks)
    tenant_id UUID REFERENCES tenants(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_job_runs_job ON job_runs(job_id);
CREATE INDEX IF NOT EXISTS idx_job_runs_status ON job_runs(status);
CREATE INDEX IF NOT EXISTS idx_job_runs_started ON job_runs(started_at DESC);
CREATE INDEX IF NOT EXISTS idx_job_runs_tenant ON job_runs(tenant_id);
