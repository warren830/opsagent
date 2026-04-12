You are working on Ops, a multi-cloud infrastructure operations platform.

## Language Preference

Always communicate with the user in **Chinese (中文)**. Code, comments, and config files remain in English.

## Reference Projects

- **kolya-br-proxy (kbp)** at `/Users/kolya/kolya-projects/kolya-br-proxy` — reference for IaC, deploy scripts, k8s patterns, security
- **opsagent** — reference for feature ideas (many features are mock/placeholder)

## Project Structure

```
ops/
├── frontend/          # Nuxt 3 + shadcn-vue + Tailwind CSS (TypeScript)
├── backend/           # Rust + Axum + SQLx + PostgreSQL
├── iac/               # Terraform modules (VPC, EKS+Karpenter, RDS, WAF)
├── k8s/               # Kubernetes manifests + Helm infra + deploy scripts
├── scripts/           # All scripts (deploy, build, local dev)
│   ├── deploy-all.sh      # One-click full deployment (4 steps)
│   ├── build-and-push.sh  # Docker build & ECR push
│   ├── deploy-to-existing.sh  # Deploy to existing cluster
│   ├── destroy.sh         # Tear down everything
│   ├── local-dev.sh       # Local dev startup
│   └── create-user.sh     # Create user utility
├── docker-compose.yml # Local dev (PostgreSQL + Redis)
├── Dockerfile.backend # Multi-stage Rust build
└── Dockerfile.frontend# Multi-stage Nuxt build
```

## Development Commands

### Backend (Rust)
```bash
cd backend
cargo run          # Start dev server on :3080
cargo test         # Run tests
cargo clippy       # Lint
```

### Frontend (Nuxt)
```bash
cd frontend
npm run dev        # Start dev server on :3000
npm run build      # Build for production
npm run lint       # Lint
```

### Local Development
```bash
./scripts/local-dev.sh    # Start everything (PostgreSQL + backend + frontend)
docker compose up -d      # Start PostgreSQL + Redis only
```

### Deployment
```bash
./scripts/deploy-all.sh           # Full deployment (terraform → helm → docker → k8s)
./scripts/deploy-all.sh --step 1  # Terraform only
./scripts/deploy-to-existing.sh   # App-only deployment to existing cluster
./scripts/destroy.sh              # Tear down everything
```

## Coding Standards

### Rust
- Use `thiserror` for error types, `anyhow` for error propagation
- All handlers return `Result<Json<T>, AppError>`
- Use SQLx compile-time checked queries where possible
- Follow Clippy suggestions

### TypeScript (Frontend)
- Vue 3 Composition API with `<script setup lang="ts">`
- Use composables for shared logic
- All API calls through `useApi()` composable
- i18n: all user-visible strings must use `$t('key')`

## Security Rules
- Never store secrets in code or .env files in production
- All sensitive values go to AWS Secrets Manager (synced via External Secrets Operator)
- SQLx parameterized queries only (no string interpolation in SQL)
- Validate all inputs on both frontend (Zod) and backend (serde validators)
- API keys from user input → encrypted → stored in Secrets Manager, never in DB

## Access Control Checklist (MUST check for every feature)

Every API endpoint and UI action must enforce the 3-tier access model:

1. **Read vs Write**: `user_account_access.role` determines access level
   - `admin` grant → full read/write to account resources
   - `readonly` grant → read only, block create/update/delete
   - `super_admin` / `tenant_admin` (own tenant) → always write
   - Use `can_write_account()` for write operations, `get_accessible_account_ids()` for reads
2. **Scope filtering**: List endpoints must filter by accessible accounts (not just tenant_id)
   - `tenant_admin` sees: tenant accounts UNION explicitly granted accounts
   - Regular user sees: only explicitly granted accounts
3. **New model fields**: When adding fields to a Rust model, check if ALL related structs need it
   - e.g., adding `is_active` to `User` → also add to `UserInfo` (the serialized response DTO)
   - e.g., adding `tenant_id` to `CreateRequest` → also add to `UpdateRequest` AND the UPDATE SQL
4. **Frontend consistency**: Every backend field must be reflected in the frontend interface AND the form
   - If API returns `is_active`, the TypeScript interface must declare it
   - If a form sends `tenant_id`, the handler must accept and persist it

## Diagrams
- All flowcharts, architecture diagrams, and sequence diagrams use **Mermaid** format
- Embed in Markdown files or code comments where appropriate

## Frontend Design
- Use polished, modern UI components (shadcn-vue + Radix Vue + Tailwind)
- Keep layouts **clean and minimal** — no visual clutter
- Apply trending effects: subtle transitions, glassmorphism, smooth hover animations, skeleton loaders, gradient accents
- Reference opsagent for feature structure; implement real functionality (not placeholder)

## Architecture Notes
- **Infrastructure**: EKS + Karpenter (ARM64 Graviton), NOT ECS
- **Frontend deployment**: Nuxt SSR on EKS (not S3+CloudFront)
- **Secrets**: AWS Secrets Manager → ESO → K8s Secrets → Pod env vars
- **Config**: .env.example for local only; k8s ConfigMap + Secrets for all other envs
- **Multi-tenancy**: tenant_id on all resources; RLS-style isolation
- **Alicloud/Azure**: mock mode (no real accounts), credentials still in Secrets Manager

## User Preferences

### Communication
- **Language**: Always Chinese (中文) for all conversations; code/comments/config stay English
- **Style**: Be concise and direct — avoid verbose audit-style documents or lengthy summaries
- **Opinions**: Give bold, expert-level functional opinions; don't be cautious or hedge

### Design Philosophy
- **UI Theme**: Grafana-inspired dark theme — data-dense, colorful, professional
- **Components**: Compact sizing (h-8 buttons/inputs, text-xs body, text-[11px] labels) — never big/clumsy
- **Effects**: Subtle transitions, glassmorphism, smooth hover animations, skeleton loaders
- **Color palette**: Dark backgrounds (#111217/#181b1f), orange primary (#FF6600), info blue, success green, warning amber

### AI Agent Philosophy
- Don't over-constrain AI agents with rigid command structures
- Give directional guidance, let agents decide specifics
- `.claude/commands` should be loose guidelines, not strict templates

### Feature Scope
- Implement ALL features — never suggest cutting or deferring features
- Replace all mock/placeholder/fake functionality with real integrations
- Skills integration uses real `npx skills` CLI, not fake database CRUD
- Chat integration uses real Claude CLI subprocess, not placeholder responses

## Ops Internal APIs

The AI agent discovers infrastructure context dynamically via internal APIs. **Never hardcode account IDs, cluster names, regions, or endpoints** — always query at runtime.

### Authentication

All protected APIs require `Authorization: Bearer $TOKEN`. Obtain a token via:
```bash
curl -s http://localhost:3080/api/auth/login -H "Content-Type: application/json" \
  -d '{"username":"...","password":"..."}' | jq -r .token
```

### Discovery APIs

| API | Purpose |
|-----|---------|
| `GET /api/accounts` | List all cloud accounts (provider, account_id, regions, role_arn) |
| `POST /api/accounts/sync` | Refresh org accounts from AWS Organizations |
| `GET /api/clusters` | List all clusters (name, cloud, region, status, config with endpoint/CA) |
| `POST /api/clusters/discover` | Refresh EKS clusters across all accounts & regions |
| `GET /api/telemetry` | Get observability endpoints (Grafana/Mimir/Loki/Tempo URLs + auth) |
| `GET /api/issues` | List alerts/issues (severity, labels, timestamps) |
| `GET /api/issues/{id}` | Get single issue with full metadata |
| `GET /api/resources/dashboard` | Security findings summary |

### Rollout Management (Argo Rollouts)

| API | Purpose |
|-----|---------|
| `GET /api/clusters/{id}/rollouts` | List rollouts on a cluster |
| `GET /api/clusters/{id}/rollouts/{ns}/{name}` | Get rollout detail (strategy, steps, status) |
| `POST /api/clusters/{id}/rollouts/{ns}/{name}/promote` | Promote a canary/bluegreen rollout |
| `POST /api/clusters/{id}/rollouts/{ns}/{name}/rollback` | Rollback a rollout |
| `POST /api/mcp/rollouts` | MCP JSON-RPC endpoint for AI agent rollout tools |

### Workflow: Investigating an Issue

1. **Get context**: Call `/api/telemetry` for observability endpoints, `/api/clusters` for cluster info, `/api/accounts` for cloud credentials context
2. **Identify runtime**: Use cluster info + service name to determine if it runs on EKS, ECS, EC2, or a managed service
3. **Query telemetry**: Based on runtime — EKS uses Mimir/Loki/Tempo (endpoints from `/api/telemetry`), everything else uses CloudWatch/X-Ray
4. **Correlate & report**: Combine metrics, logs, traces into a concise RCA

### Runtime → Telemetry Mapping

| Runtime | Metrics | Logs | Traces |
|---------|---------|------|--------|
| **EKS** | Mimir (PromQL) | Loki (LogQL) | Tempo |
| **ECS/EC2/RDS/Lambda** | CloudWatch | CloudWatch Logs | X-Ray |

The agent should determine which stack to query based on where the service runs — never assume. Stream findings live as you work.
