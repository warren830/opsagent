# OpenOps

AI-powered multi-cloud infrastructure operations platform. A modern, frontend-backend separated rebuild of OpsAgent with enterprise-grade security, multi-tenancy, and production-ready deployment.

## Features

- **Multi-Cloud**: AWS native, Alicloud (mock), Azure (mock)
- **Multi-Tenant**: Per-tenant skills, MCP servers, cloud accounts, and provider isolation
- **Claude Integration**: Real-time streaming chat with Claude Code CLI, persistent sessions
- **Admin Console**: Modern web UI (Nuxt 3 + shadcn-vue) for managing all resources
- **Role-Based Access**: Super Admin (full) vs Tenant Admin (team-scoped)
- **Dark/Light Theme**: System-aware theme switching
- **i18n**: English and Chinese language support
- **Security**: JWT auth, bcrypt passwords, AWS Secrets Manager + ESO, CORS/CSP/CSRF, WAF
- **Infrastructure as Code**: Terraform modules for AWS (EKS + Karpenter + Aurora + WAF)

## Architecture

```
                    ┌──────────────────────────────────────────┐
                    │            AWS EKS Cluster               │
                    │  ┌──────────┐        ┌──────────────┐   │
 Browser ──ALB───▶  │  │  Nuxt 3  │──API──▶│  Rust (Axum) │   │
                    │  │ Frontend │        │   Backend    │   │
                    │  └──────────┘        └──────┬───────┘   │
                    │                             │           │
                    │              ┌───────────────┼────────┐  │
                    │              │               ▼        │  │
                    │              │  Aurora PostgreSQL v2   │  │
                    │              │  AWS Secrets Manager    │  │
                    │              └────────────────────────┘  │
                    └──────────────────────────────────────────┘
```

## Quick Start (Local)

```bash
# One-command local dev
./scripts/local-dev.sh

# Or step by step:
docker compose up -d              # PostgreSQL + Redis
cd backend && cargo run           # Rust API on :3080
cd frontend && npm install && npm run dev  # Nuxt on :3000

# Open: http://localhost:3000 (admin / admin123)
```

## Project Structure

```
openops/
├── frontend/            # Nuxt 3 + shadcn-vue + Tailwind CSS (TypeScript)
├── backend/             # Rust Axum + SQLx + PostgreSQL
├── iac/                 # Terraform (VPC, EKS+Karpenter, RDS Aurora, WAF)
├── k8s/                 # K8s manifests, Helm infra, deploy scripts
├── scripts/             # Local dev utilities
├── deploy-all.sh        # One-click full deployment (4 steps)
├── build-and-push.sh    # Docker build & ECR push
├── deploy-to-existing.sh # App-only deploy to existing cluster
├── destroy.sh           # Tear down everything
├── docker-compose.yml   # Local dev (PostgreSQL + Redis)
└── .env.example         # Local dev environment template
```

## Deployment (AWS)

```bash
# Full deployment: Terraform → Helm → Docker → K8s
./deploy-all.sh

# Individual steps
./deploy-all.sh --step 1   # Terraform apply (VPC, EKS, RDS, Secrets)
./deploy-all.sh --step 2   # Helm install (ALB Controller, Karpenter, ESO, Redis)
./deploy-all.sh --step 3   # Build & push Docker images to ECR
./deploy-all.sh --step 4   # Deploy application to EKS

# App-only update (skip infra)
./deploy-to-existing.sh

# Tear down
./destroy.sh
```

## License

MIT
