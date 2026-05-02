-- W3 · Link issues back to the incident they've been promoted into.
-- A promoted issue is idempotent: re-POSTing /promote returns the same
-- incident. We store the incident_id on the issue row so we can short-
-- circuit the promotion path without adding another lookup table.
--
-- See docs/platform-evolution.md §5 and AGENT_BRIEF.md W3.

ALTER TABLE issues
    ADD COLUMN IF NOT EXISTS incident_id UUID REFERENCES incidents(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_issues_incident
    ON issues(incident_id) WHERE incident_id IS NOT NULL;
