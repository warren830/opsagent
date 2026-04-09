-- Deployment event audit log — records promote, rollback, strategy changes.
CREATE TABLE IF NOT EXISTS deployment_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    cluster_id UUID NOT NULL REFERENCES clusters(id) ON DELETE CASCADE,
    namespace VARCHAR(253) NOT NULL,
    rollout_name VARCHAR(253) NOT NULL,
    action VARCHAR(50) NOT NULL,  -- promote_step, promote_full, rollback, change_strategy
    detail JSONB NOT NULL DEFAULT '{}',
    user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    tenant_id UUID REFERENCES tenants(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_deployment_events_cluster ON deployment_events(cluster_id, created_at DESC);
CREATE INDEX idx_deployment_events_rollout ON deployment_events(cluster_id, namespace, rollout_name, created_at DESC);
