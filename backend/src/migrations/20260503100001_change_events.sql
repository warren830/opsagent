-- change_events: global change-flow stream (W10 Joint Integration).
--
-- Captures every non-trivial change to a service during and OUTSIDE incident
-- windows. `incident_timeline_events` is per-incident and disappears from
-- queries once the incident closes; `change_events` is the long-lived global
-- audit that answers "what changed for service X in the last 30 min" even
-- when no incident was open.
--
-- See docs/platform-evolution.md §2.3 (IA) and §6.1 decision #5.

CREATE TABLE IF NOT EXISTS change_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID REFERENCES tenants(id) ON DELETE CASCADE,
    kind VARCHAR(64) NOT NULL,                       -- deploy | rollback | config | feature_flag | slo_burn | manual | catalog_import
    service_id UUID REFERENCES catalog_entities(id) ON DELETE SET NULL,
    actor JSONB NOT NULL,                             -- { type: 'user'|'system'|'agent', id, display_name }
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    summary TEXT NOT NULL,
    payload JSONB NOT NULL DEFAULT '{}',
    correlation_id TEXT,
    source VARCHAR(32) NOT NULL,                      -- argocd | rollout_api | slo_burn | manual | import_run
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_change_events_service_time
    ON change_events(service_id, occurred_at DESC);
CREATE INDEX IF NOT EXISTS idx_change_events_tenant_time
    ON change_events(tenant_id, occurred_at DESC);
CREATE INDEX IF NOT EXISTS idx_change_events_kind
    ON change_events(kind);
