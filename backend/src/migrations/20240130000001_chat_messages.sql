-- Chat messages — persists individual messages for session history
CREATE TABLE IF NOT EXISTS chat_messages (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    session_id VARCHAR(255) NOT NULL,  -- claude_session_id (string key, not FK)
    role VARCHAR(20) NOT NULL,          -- 'user' | 'assistant'
    content TEXT NOT NULL DEFAULT '',
    msg_type VARCHAR(20) NOT NULL DEFAULT 'text',  -- 'text' | 'thinking' | 'tool_use' | 'tool_result' | 'error'
    tool_name VARCHAR(100),
    images JSONB,
    duration_ms BIGINT,
    seq INT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_chat_messages_session ON chat_messages(session_id, seq);
