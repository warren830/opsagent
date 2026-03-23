# OpsAgent

AI-powered multi-cloud infrastructure query assistant. Teams send natural language questions via Feishu, Slack, or Teams, and OpsAgent uses Claude to query AWS, Alicloud, Kubernetes, and more.

## Features

- **Multi-Cloud**: AWS (EKS, S3, EC2, RDS), Alicloud (ECS, ACK, SLB, RDS, OSS), Azure, GCP
- **Multi-Tenant**: Per-team glossary, skills, knowledge, and cloud account isolation
- **IM Platforms**: Feishu/Lark, Slack, Microsoft Teams
- **Streaming Chat**: Real-time token streaming with thinking/tool-use indicators
- **Admin Console**: Web UI for managing glossary, accounts, knowledge, skills, tenants, users
- **Role-Based Access**: Super Admin (full) vs Tenant Admin (team-scoped)
- **Kubernetes**: Auto-discovery of EKS clusters, pre-configured kubeconfig, cross-cluster queries
- **Scheduled Jobs**: Cron-based automated queries with multi-channel delivery
- **MCP Plugins**: Confluence, Jira, GitHub integration
- **LLM Providers**: Amazon Bedrock (default), Anthropic API, LiteLLM Gateway, Vertex AI, Azure Foundry

## Quick Start

```bash
# Install dependencies
cd bot && npm install

# Build
npm run build

# Create first admin user
./scripts/create-user.sh

# Start server
PORT=3978 node dist/index.js

# Open admin console
open http://localhost:3978/admin
```

## Architecture

```
IM (Feishu/Slack/Teams) ──webhook──> OpsAgent Server
                                        |
                                   TenantResolver (channel → team mapping)
                                        |
                                   ClaudeClient (spawn Claude CLI)
                                        |
                                   CLAUDE.md (tenant-scoped system prompt)
                                        |
                                   Claude Code (aws cli, kubectl, aliyun cli)
```

## Configuration

All configs in `config/` (seeded to `knowledge/_config/` on first boot):

| File | Purpose |
|------|---------|
| `accounts.yaml` | AWS accounts, cross-account roles, multi-cloud |
| `glossary.yaml` | Company terminology mapping |
| `tenants.yaml` | Team-to-channel mapping, per-tenant cloud accounts |
| `users.yaml` | Admin users (bcrypt password hashes) |
| `clusters.yaml` | Kubernetes cluster discovery settings |
| `skills.yaml` | Composable operation guides |
| `platforms.yaml` | IM platform credentials |
| `providers.yaml` | LLM backend configuration |
| `plugins.yaml` | MCP plugin (Confluence, Jira) settings |
| `scheduled-jobs.yaml` | Cron-based automated queries |

## Admin Roles

| Role | Access |
|------|--------|
| `super_admin` | All 12 tabs, full CRUD, user management |
| `tenant_admin` | 6 tabs (Chat, Glossary, Accounts, Skills, Knowledge, Clusters), scoped to own tenant |

## API Endpoints

- `GET /health` — Health check with dependency status
- `POST /api/messages/{platform}` — IM webhook (feishu, slack, teams)
- `GET /admin` — Admin console
- `POST /admin/api/auth/login` — Login
- `GET /admin/api/auth/me` — Current user info
- `PUT /admin/api/auth/change-password` — Self password change
- `GET/PUT /admin/api/{resource}` — CRUD for glossary, accounts, skills, etc.
- `GET/PUT /admin/api/tenants/{id}/{resource}` — Per-tenant resources
- `GET/POST/DELETE /admin/api/users` — User management

## Security

- bcrypt password hashing (cost 10)
- HttpOnly + SameSite=Strict + Secure session cookies
- Login rate limiting (5 attempts / 15 min)
- Chat rate limiting (2 concurrent queries / user)
- Slack webhook signature verification
- Request body size limit (10MB)
- Path traversal protection on all file operations
- Tenant scope enforcement (cannot access other tenants' data)
- CORS disabled by default (same-origin only)
- Graceful shutdown on SIGTERM/SIGINT

## Testing

```bash
# Unit tests (48 tests)
cd bot && node --test --require ts-node/register src/__tests__/*.test.ts

# E2E tests (55 tests via Playwright)
PORT=3999 node dist/index.js &
node tests/admin-ui.spec.mjs
```

## License

Internal use only.
