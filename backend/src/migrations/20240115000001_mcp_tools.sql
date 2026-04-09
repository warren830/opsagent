-- Store discovered tools from MCP servers (populated by test/discovery)
ALTER TABLE mcp_servers ADD COLUMN IF NOT EXISTS tools JSONB NOT NULL DEFAULT '[]';
