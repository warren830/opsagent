You are working on OpenOps, a multi-cloud infrastructure operations platform.

## Language Preference

Always communicate with the user in **Chinese (中文)**. Code, comments, and config files remain in English.

## Reference Projects

- **kolya-br-proxy (kbp)** at `/Users/kolya/kolya-projects/kolya-br-proxy` — reference for IaC, deploy scripts, k8s patterns, security
- **opsagent** — reference for feature ideas (many features are mock/placeholder)

## Project Structure

```
openops/
├── frontend/          # Nuxt 3 + shadcn-vue + Tailwind CSS (TypeScript)
├── backend/           # Rust + Axum + SQLx + PostgreSQL
├── iac/               # Terraform modules (VPC, EKS+Karpenter, RDS, WAF)
├── k8s/               # Kubernetes manifests + Helm infra + deploy scripts
├── scripts/           # Local dev utilities (local-dev.sh, create-user.sh)
├── deploy-all.sh      # One-click full deployment (4 steps)
├── build-and-push.sh  # Docker build & ECR push
├── deploy-to-existing.sh  # Deploy to existing cluster
├── destroy.sh         # Tear down everything
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
./deploy-all.sh           # Full deployment (terraform → helm → docker → k8s)
./deploy-all.sh --step 1  # Terraform only
./deploy-to-existing.sh   # App-only deployment to existing cluster
./destroy.sh              # Tear down everything
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

## Observability & Intelligent Operations

The platform manages workloads across **multiple runtimes**: EKS (Kubernetes), ECS (Fargate/EC2), standalone EC2, and AWS managed services (RDS, ElastiCache, OpenSearch, etc.). Each runtime has its own telemetry stack. The agent MUST discover where a service runs before querying observability data.

### Step 0 — Discover Infrastructure Topology

Before any prediction or RCA, determine where the target service lives:

```bash
# 1. Check the OpenOps telemetry config (Grafana/Mimir/Loki/Tempo endpoints + auth)
curl -s -H "Authorization: Bearer $TOKEN" http://localhost:3080/api/telemetry | jq .

# 2. Check clusters registered in OpenOps
curl -s -H "Authorization: Bearer $TOKEN" http://localhost:3080/api/clusters | jq '.[] | {name, cloud, type, region}'

# 3. For a given service, figure out the runtime:
#    - EKS: kubectl get deploy,sts -A | grep <service>
#    - ECS: aws ecs list-services --cluster <cluster> | grep <service>
#    - EC2: aws ec2 describe-instances --filters "Name=tag:Name,Values=*<service>*"
#    - RDS: aws rds describe-db-clusters / describe-db-instances
```

### Decision Matrix — Where to Find Data

| Runtime | Metrics | Logs | Traces |
|---------|---------|------|--------|
| **EKS pods** | Mimir (PromQL) via Alloy scrape | Loki (LogQL) via Alloy | Tempo (via OTLP/Alloy) |
| **ECS tasks** | CloudWatch `ECS/ContainerInsights` | CloudWatch Logs (`/ecs/<service>`) | AWS X-Ray |
| **EC2 instances** | CloudWatch `AWS/EC2` + CW Agent `CWAgent` | CloudWatch Logs (via CW Agent) | X-Ray (if instrumented) |
| **RDS/Aurora** | CloudWatch `AWS/RDS` | RDS Performance Insights (if enabled) | N/A |
| **ALB/NLB** | CloudWatch `AWS/ApplicationELB` or `AWS/NetworkELB` | ALB access logs (S3) | N/A |
| **Lambda** | CloudWatch `AWS/Lambda` | CloudWatch Logs (`/aws/lambda/<fn>`) | X-Ray (built-in) |
| **OpenSearch** | CloudWatch `AWS/ES` | CloudWatch Logs (slow logs) | N/A |
| **SQS/SNS** | CloudWatch `AWS/SQS`, `AWS/SNS` | N/A | N/A |

### Leg 1 — Predictive Risk Detection

Two independent prediction engines. Use both — they cover different scopes.

**1A. CloudWatch Anomaly Detection** — AWS managed services + ECS + EC2

Covers everything with CloudWatch metrics. ML-based seasonal baseline.

```bash
# List active anomaly detectors
aws cloudwatch describe-anomaly-detectors --output json | jq '.AnomalyDetectors[] | {Namespace, MetricName, Stat}'

# Query metric with anomaly band (2σ deviation)
aws cloudwatch get-metric-data --metric-data-queries '[
  {"Id":"m1","MetricStat":{"Metric":{"Namespace":"AWS/RDS","MetricName":"CPUUtilization","Dimensions":[{"Name":"DBClusterIdentifier","Value":"CLUSTER"}]},"Period":300,"Stat":"Average"}},
  {"Id":"ad1","Expression":"ANOMALY_DETECTION_BAND(m1,2)"}
]' --start-time $(date -u -v-6H +%Y-%m-%dT%H:%M:%S) --end-time $(date -u +%Y-%m-%dT%H:%M:%S)

# Current alarms in ALARM state
aws cloudwatch describe-alarms --state-value ALARM --output table
```

**1B. Mimir predict_linear()** — EKS pod/container/node metrics

Only works if Mimir is configured. Get the endpoint from `/api/telemetry`.

```bash
# Disk full within 4h?
curl -s '<MIMIR_URL>/api/v1/query' --data-urlencode \
  'query=predict_linear(node_filesystem_avail_bytes[1h], 14400) < 0' | jq .data.result

# Pod OOM within 2h?
curl -s '<MIMIR_URL>/api/v1/query' --data-urlencode \
  'query=predict_linear(container_memory_working_set_bytes[30m], 7200) > container_spec_memory_limit_bytes' | jq .data.result

# CPU saturation within 1h?
curl -s '<MIMIR_URL>/api/v1/query' --data-urlencode \
  'query=predict_linear(rate(container_cpu_usage_seconds_total[5m])[30m:1m], 3600) > 0.9' | jq .data.result
```

For Grafana Cloud mode (user_id auth): `curl -s -u '<USER_ID>:$GRAFANA_API_TOKEN' <URL>`.

### Leg 2 — Automatic RCA (Root Cause Analysis)

When an alert fires (Grafana/Datadog/Dynatrace → webhook → Issue created), perform RCA automatically. **Stream findings live** — the user watches your output in real-time.

**Step 1 — Parse alert**: Read Issue title, severity, labels, annotations, fingerprint, startsAt. Determine affected service, namespace/cluster, time window (±15 min).

**Step 2 — Identify runtime**: Use the discovery commands above. This determines which telemetry stack to query.

**Step 3 — Metrics**: Query the appropriate source based on runtime:

```bash
# For EKS pods → Mimir
curl -s '<MIMIR_URL>/api/v1/query_range' \
  --data-urlencode 'query=rate(container_cpu_usage_seconds_total{namespace="<NS>",pod=~"<SERVICE>.*"}[5m])' \
  --data-urlencode 'start=<ALERT-15m>' --data-urlencode 'end=<ALERT+15m>' --data-urlencode 'step=60'

# For ECS/EC2/RDS → CloudWatch
aws cloudwatch get-metric-statistics --namespace <NS> --metric-name <METRIC> \
  --dimensions Name=<DIM>,Value=<VAL> \
  --start-time <ALERT-15m> --end-time <ALERT+15m> --period 60 --statistics Average Maximum
```

**Step 4 — Logs**: Query the appropriate source:

```bash
# For EKS pods → Loki
curl -s -G '<LOKI_URL>/loki/api/v1/query_range' \
  --data-urlencode 'query={namespace="<NS>"} |= "error" or |= "fatal" or |= "panic"' \
  --data-urlencode 'start=<ALERT-15m>' --data-urlencode 'end=<ALERT+15m>' --data-urlencode 'limit=100'

# For ECS/Lambda/EC2 → CloudWatch Logs
aws logs filter-log-events --log-group-name <LOG_GROUP> \
  --start-time <EPOCH_MS> --end-time <EPOCH_MS> \
  --filter-pattern "?ERROR ?error ?Exception ?FATAL"
```

**Step 5 — Traces** (if available):

```bash
# For EKS → Tempo
curl -s '<TEMPO_URL>/api/search?tags=service.name%3D<SERVICE>&minDuration=1s&limit=10&start=<EPOCH-15m>&end=<EPOCH+15m>'
# Then: curl -s '<TEMPO_URL>/api/traces/<TRACE_ID>'

# For ECS/Lambda → X-Ray
aws xray get-trace-summaries --start-time <ALERT-15m> --end-time <ALERT+15m> \
  --filter-expression 'service("<SERVICE>") AND responsetime > 1'
```

**Step 6 — Correlate & Report**:

```
## RCA Report
**Alert**: <title>
**Time**: <startsAt>
**Runtime**: EKS / ECS / EC2 / RDS (auto-detected)
**Root Cause**: <1-2 sentence explanation>
**Evidence**:
  - Metrics: <what was abnormal, actual vs expected>
  - Logs: <relevant error messages with timestamps>
  - Traces: <trace IDs showing failure path, if available>
**Impact**: <what was affected, blast radius>
**Recommendation**: <immediate action + longer-term fix>
```

CRITICAL: Stream each step's output as you execute it. The user is watching a live window — show progress, not silence. If a data source is unavailable or returns nothing, say so immediately and move to the next source.
