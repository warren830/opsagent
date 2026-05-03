-- Per-tenant webhook secrets for alerting providers (grafana/datadog/dynatrace).
-- Each enabled row holds a bcrypt-hashed shared token. The alerts handlers
-- verify the `X-Webhook-Token` / `X-Webhook-Signature` / `Authorization: Bearer`
-- header against the full set of enabled secrets (short-circuit on first match)
-- and bind the inbound alert to that tenant.
CREATE TABLE IF NOT EXISTS webhook_secrets (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    provider VARCHAR(32) NOT NULL CHECK (provider IN ('grafana','datadog','dynatrace')),
    secret_hash TEXT NOT NULL,  -- bcrypt of the shared token
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (tenant_id, provider)
);
CREATE INDEX IF NOT EXISTS idx_webhook_secrets_provider ON webhook_secrets(provider) WHERE enabled;
