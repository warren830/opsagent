-- W1 Incident Command Center core schema.
-- See docs/platform-evolution.md §5 (§5.2 data model, §5.3 state machine).
-- W0 (20260502000001_component_spec_lock.sql) already installed:
--   catalog_entities, issues.affected_component_ids, claude_sessions.context_type/context_id.

-- Tenant-wide monotonic number sequence for human-friendly INC-YYYY-NNNN.
CREATE SEQUENCE IF NOT EXISTS incident_number_seq START 1;

-- ----------------------------------------------------------------------------
-- incidents
-- ----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS incidents (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID REFERENCES tenants(id) ON DELETE CASCADE,
    number BIGINT NOT NULL DEFAULT nextval('incident_number_seq'),
    title TEXT NOT NULL,
    severity VARCHAR(8) NOT NULL
        CHECK (severity IN ('sev1','sev2','sev3','sev4')),
    status VARCHAR(32) NOT NULL
        CHECK (status IN (
            'triggered','acknowledged','investigating','identified',
            'mitigated','resolved','postmortem_draft','postmortem_published','closed'
        )),
    commander_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    scribe_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    impact_summary TEXT,
    affected_component_ids UUID[] NOT NULL DEFAULT '{}',
    affected_customer_tier VARCHAR(32),
    detection_source VARCHAR(32) NOT NULL
        CHECK (detection_source IN ('alert','manual','slo_burn','chaos','synthetic')),
    source_issue_id UUID REFERENCES issues(id) ON DELETE SET NULL,
    started_at TIMESTAMPTZ NOT NULL,
    detected_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    acknowledged_at TIMESTAMPTZ,
    mitigated_at TIMESTAMPTZ,
    resolved_at TIMESTAMPTZ,
    closed_at TIMESTAMPTZ,
    war_room_channel_ref JSONB,
    bridge_url TEXT,
    jira_key VARCHAR(64),
    postmortem_doc_ref JSONB,
    root_cause TEXT,
    root_cause_category VARCHAR(32),
    labels JSONB NOT NULL DEFAULT '{}',
    slo_budget_burn JSONB,
    merged_into_id UUID REFERENCES incidents(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (tenant_id, number)
);

CREATE INDEX IF NOT EXISTS idx_incidents_status
    ON incidents(status) WHERE status <> 'closed';
CREATE INDEX IF NOT EXISTS idx_incidents_tenant_active
    ON incidents(tenant_id, status) WHERE status <> 'closed';
CREATE INDEX IF NOT EXISTS idx_incidents_components
    ON incidents USING GIN (affected_component_ids);
CREATE INDEX IF NOT EXISTS idx_incidents_source_issue
    ON incidents(source_issue_id) WHERE source_issue_id IS NOT NULL;

-- ----------------------------------------------------------------------------
-- incident_timeline_events
-- ----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS incident_timeline_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    incident_id UUID NOT NULL REFERENCES incidents(id) ON DELETE CASCADE,
    kind VARCHAR(64) NOT NULL,
    actor JSONB NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    service_id UUID REFERENCES catalog_entities(id) ON DELETE SET NULL,
    summary TEXT NOT NULL,
    payload JSONB NOT NULL DEFAULT '{}',
    correlation_id TEXT
);

CREATE INDEX IF NOT EXISTS idx_timeline_incident_time
    ON incident_timeline_events(incident_id, occurred_at DESC);
CREATE INDEX IF NOT EXISTS idx_timeline_kind
    ON incident_timeline_events(kind);

-- ----------------------------------------------------------------------------
-- incident_participants
-- ----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS incident_participants (
    incident_id UUID NOT NULL REFERENCES incidents(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role VARCHAR(32) NOT NULL
        CHECK (role IN ('commander','scribe','responder','observer','stakeholder')),
    joined_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    left_at TIMESTAMPTZ,
    added_via VARCHAR(32) NOT NULL DEFAULT 'manual_invite'
        CHECK (added_via IN ('on_call_auto','manual_invite','self_join')),
    PRIMARY KEY (incident_id, user_id, role)
);

-- ----------------------------------------------------------------------------
-- incident_severity_history
-- ----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS incident_severity_history (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    incident_id UUID NOT NULL REFERENCES incidents(id) ON DELETE CASCADE,
    from_severity VARCHAR(8),
    to_severity VARCHAR(8) NOT NULL,
    changed_by UUID REFERENCES users(id) ON DELETE SET NULL,
    reason TEXT,
    changed_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ----------------------------------------------------------------------------
-- incident_updates (stakeholder communications)
-- ----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS incident_updates (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    incident_id UUID NOT NULL REFERENCES incidents(id) ON DELETE CASCADE,
    author_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    audience VARCHAR(32) NOT NULL
        CHECK (audience IN ('internal','customers','stakeholders','status_page')),
    status_at_time VARCHAR(32) NOT NULL,
    body_markdown TEXT NOT NULL,
    published_at TIMESTAMPTZ,
    pushed_to JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
