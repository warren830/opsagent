-- W4 burn-event flap suppression.
--
-- The Mimir alertmanager pipeline (via our generated MWMBR rules) can fire
-- the same (slo_id, window) burst repeatedly as the alert oscillates around
-- the threshold. We want a single OPEN burn row at a time for each pairing;
-- once the alert resolves, `resolved_at` is set and a new row can be inserted
-- on the next firing.
--
-- Implemented with a partial UNIQUE index on the open rows only. The
-- `slo_burn_events` INSERT uses `ON CONFLICT DO NOTHING` so alertmanager
-- retries collapse into a no-op without raising an error.
--
-- Note: we intentionally do NOT include `severity` in the uniqueness tuple —
-- the rule generator produces one alert per (window) pair, and a window is
-- the natural dedup boundary operators expect (e.g. "one 1h burn per SLO").

CREATE UNIQUE INDEX IF NOT EXISTS uniq_slo_burn_open
    ON slo_burn_events (slo_id, window)
    WHERE resolved_at IS NULL;
