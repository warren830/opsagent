-- W1 core schema for the SLO engine (feat/slo-mvp).
-- Three tables: slos (definitions), error_budget_snapshots (5-min history),
-- slo_burn_events (Multi-Window Multi-Burn-Rate alerts linked to issues).
--
-- See:
--   - docs/platform-evolution.md §4.2 (data model) and §4.4 (MWMBR policy).
--   - AGENT_BRIEF.md W1 task list.
--
-- NOTE: catalog_entities was introduced in 20260502000001_component_spec_lock.sql.
-- The issues.slo_id column also exists from that migration but is not wired as
-- a real FK yet (bootstrap chicken-and-egg); a future migration may add the FK
-- once the module ownership boundaries stabilise.

-- ---------------------------------------------------------------------------
-- slos: user-defined SLO definitions.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS slos (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    component_id UUID REFERENCES catalog_entities(id) ON DELETE SET NULL,
    name VARCHAR(128) NOT NULL,
    description TEXT,
    sli_type VARCHAR(32) NOT NULL
        CHECK (sli_type IN ('availability', 'latency', 'error_rate', 'custom')),
    good_events_query TEXT NOT NULL,
    total_events_query TEXT NOT NULL,
    objective_pct DOUBLE PRECISION NOT NULL
        CHECK (objective_pct > 0 AND objective_pct < 100),
    window_days INT NOT NULL
        CHECK (window_days IN (7, 28, 30)),
    burn_rate_policy VARCHAR(32) NOT NULL DEFAULT 'mwmbr_default',
    labels JSONB NOT NULL DEFAULT '{}',
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    recording_rules_hash VARCHAR(64),
    created_by UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (tenant_id, name)
);

CREATE INDEX IF NOT EXISTS idx_slos_tenant ON slos(tenant_id);
CREATE INDEX IF NOT EXISTS idx_slos_component
    ON slos(component_id)
    WHERE component_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_slos_enabled
    ON slos(enabled)
    WHERE enabled;

-- ---------------------------------------------------------------------------
-- error_budget_snapshots: periodic snapshots for historical budget charts.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS error_budget_snapshots (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slo_id UUID NOT NULL REFERENCES slos(id) ON DELETE CASCADE,
    captured_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    window_start TIMESTAMPTZ NOT NULL,
    window_end TIMESTAMPTZ NOT NULL,
    sli_achieved_pct DOUBLE PRECISION NOT NULL,
    budget_total_minutes DOUBLE PRECISION NOT NULL,
    budget_consumed_minutes DOUBLE PRECISION NOT NULL,
    budget_remaining_pct DOUBLE PRECISION NOT NULL,
    burn_rate_1h DOUBLE PRECISION,
    burn_rate_6h DOUBLE PRECISION,
    burn_rate_24h DOUBLE PRECISION,
    burn_rate_3d DOUBLE PRECISION
);

CREATE INDEX IF NOT EXISTS idx_budget_snapshots_slo_time
    ON error_budget_snapshots(slo_id, captured_at DESC);

-- ---------------------------------------------------------------------------
-- slo_burn_events: MWMBR burn-rate alerts, optionally linked to an issue.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS slo_burn_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slo_id UUID NOT NULL REFERENCES slos(id) ON DELETE CASCADE,
    severity VARCHAR(16) NOT NULL
        CHECK (severity IN ('page', 'ticket')),
    window VARCHAR(16) NOT NULL,
    burn_rate DOUBLE PRECISION NOT NULL,
    threshold DOUBLE PRECISION NOT NULL,
    triggered_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    resolved_at TIMESTAMPTZ,
    issue_id UUID REFERENCES issues(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_slo_burn_slo
    ON slo_burn_events(slo_id, triggered_at DESC);
CREATE INDEX IF NOT EXISTS idx_slo_burn_unresolved
    ON slo_burn_events(slo_id)
    WHERE resolved_at IS NULL;
