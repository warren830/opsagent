-- Notifications
CREATE TABLE IF NOT EXISTS notifications (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id     UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    tenant_id   UUID REFERENCES tenants(id) ON DELETE SET NULL,
    event_type  TEXT NOT NULL DEFAULT 'info',
    title       TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    payload     JSONB NOT NULL DEFAULT '{}',
    related_id  UUID,
    is_read     BOOLEAN NOT NULL DEFAULT false,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_notifications_user_id ON notifications (user_id);
CREATE INDEX IF NOT EXISTS idx_notifications_user_unread ON notifications (user_id, is_read) WHERE is_read = false;

-- Custom RBAC roles
CREATE TABLE IF NOT EXISTS roles (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name        TEXT NOT NULL,
    label       TEXT NOT NULL DEFAULT '',
    description TEXT NOT NULL DEFAULT '',
    permissions JSONB NOT NULL DEFAULT '[]',
    is_system   BOOLEAN NOT NULL DEFAULT false,
    tenant_id   UUID REFERENCES tenants(id) ON DELETE CASCADE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (name, tenant_id)
);

-- Entra ID SSO connections
CREATE TABLE IF NOT EXISTS entra_id_connections (
    id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name             TEXT NOT NULL,
    entra_tenant_id  TEXT NOT NULL,
    client_id        TEXT NOT NULL,
    client_secret    TEXT NOT NULL,
    tenant_id        UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    auto_provision   BOOLEAN NOT NULL DEFAULT true,
    default_role     TEXT NOT NULL DEFAULT 'user',
    enabled          BOOLEAN NOT NULL DEFAULT true,
    allowed_domains  TEXT[] NOT NULL DEFAULT '{}',
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_entra_id_connections_tenant ON entra_id_connections (tenant_id);

-- Entra group → role mappings
CREATE TABLE IF NOT EXISTS entra_group_mappings (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    group_id       TEXT NOT NULL,
    group_name     TEXT NOT NULL DEFAULT '',
    role           TEXT NOT NULL DEFAULT 'user',
    tenant_id      UUID REFERENCES tenants(id) ON DELETE CASCADE,
    account_access JSONB NOT NULL DEFAULT '[]',
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_entra_group_mappings_group_id ON entra_group_mappings (group_id);
