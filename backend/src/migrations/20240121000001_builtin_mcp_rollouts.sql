-- Seed built-in MCP server for Argo Rollouts management
INSERT INTO mcp_servers (name, transport_type, url, command, args, env, headers, enabled, visibility, description, tenant_id, user_id, created_by)
VALUES (
    'openops-rollouts',
    'http',
    'http://localhost:3080/api/mcp/rollouts',
    '', '[]'::jsonb, '{}'::jsonb, '{}'::jsonb,
    true, 'public',
    'Argo Rollouts management - list, promote, rollback deployments',
    NULL, NULL, NULL
)
ON CONFLICT DO NOTHING;
