-- OAuth authentication: add OAuth fields to users, create oauth_states and refresh_tokens tables

-- 1. Add OAuth fields to users table
ALTER TABLE users ADD COLUMN IF NOT EXISTS microsoft_id TEXT UNIQUE;
ALTER TABLE users ADD COLUMN IF NOT EXISTS cognito_sub TEXT UNIQUE;
ALTER TABLE users ADD COLUMN IF NOT EXISTS auth_method TEXT NOT NULL DEFAULT 'local';

-- Make password_hash nullable (OAuth users won't have one)
ALTER TABLE users ALTER COLUMN password_hash DROP NOT NULL;

-- 2. OAuth state management (PKCE flow, 10-min TTL)
CREATE TABLE IF NOT EXISTS oauth_states (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    state TEXT NOT NULL UNIQUE,
    provider TEXT NOT NULL,          -- 'microsoft' or 'cognito'
    code_verifier TEXT NOT NULL,     -- PKCE code_verifier (stored server-side)
    redirect_uri TEXT,               -- which redirect_uri was used
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL DEFAULT NOW() + INTERVAL '10 minutes'
);

CREATE INDEX idx_oauth_states_state ON oauth_states(state);
CREATE INDEX idx_oauth_states_expires ON oauth_states(expires_at);

-- 3. Refresh token rotation with family-based theft detection
CREATE TABLE IF NOT EXISTS refresh_tokens (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash TEXT NOT NULL UNIQUE,     -- HMAC-SHA256 hash of the JWT
    family_id UUID NOT NULL,             -- token family for theft detection
    parent_token_id UUID REFERENCES refresh_tokens(id) ON DELETE SET NULL,
    ip_address TEXT,
    user_agent TEXT,
    is_revoked BOOLEAN NOT NULL DEFAULT false,
    revoked_reason TEXT,                 -- 'logout', 'rotation', 'theft', 'revoke_all'
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX idx_refresh_tokens_user_id ON refresh_tokens(user_id);
CREATE INDEX idx_refresh_tokens_token_hash ON refresh_tokens(token_hash);
CREATE INDEX idx_refresh_tokens_family_id ON refresh_tokens(family_id);
CREATE INDEX idx_refresh_tokens_expires ON refresh_tokens(expires_at);
