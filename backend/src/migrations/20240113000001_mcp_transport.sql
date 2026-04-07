-- Add transport type, URL, headers, description to MCP servers
ALTER TABLE mcp_servers ADD COLUMN IF NOT EXISTS transport_type VARCHAR(10) NOT NULL DEFAULT 'stdio';
ALTER TABLE mcp_servers ADD COLUMN IF NOT EXISTS url TEXT;
ALTER TABLE mcp_servers ADD COLUMN IF NOT EXISTS headers JSONB NOT NULL DEFAULT '{}';
ALTER TABLE mcp_servers ADD COLUMN IF NOT EXISTS description TEXT;
