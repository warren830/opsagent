# Deployment Guide

## AWS Deployment (EKS)

### Prerequisites

| Tool | Version | Purpose |
|------|---------|---------|
| AWS CLI | v2 | Cloud operations |
| Terraform | 1.5+ | Infrastructure provisioning |
| kubectl | 1.28+ | Kubernetes management |
| Helm | 3.12+ | Chart deployment |
| Docker | 20+ | Image builds |

### Full Deployment (One Command)

```bash
./scripts/deploy-all.sh
```

This runs all 4 steps sequentially:

1. **Terraform Apply** — VPC, EKS + Karpenter, RDS Aurora, WAF, Secrets Manager, Cognito
2. **Helm Install** — ALB Controller, Karpenter, External Secrets Operator, Redis
3. **Docker Build & Push** — Build backend/frontend images, push to ECR
4. **K8s Deploy** — Apply ConfigMaps, Secrets, Deployments, Services, Ingress

### Individual Steps

```bash
./scripts/deploy-all.sh --step 1   # Terraform only
./scripts/deploy-all.sh --step 2   # Helm charts only
./scripts/deploy-all.sh --step 3   # Docker build & push only
./scripts/deploy-all.sh --step 4   # K8s application deploy only
```

### App-Only Update (Skip Infra)

For code changes that don't touch infrastructure:

```bash
./scripts/deploy-to-existing.sh
```

This builds new images, pushes to ECR, and rolls out the update on EKS.

### Build & Push Images Only

```bash
./scripts/build-and-push.sh               # Both backend + frontend
./scripts/build-and-push.sh --backend     # Backend only
./scripts/build-and-push.sh --frontend    # Frontend only
```

### Tear Down

```bash
./scripts/destroy.sh
```

> **Warning**: This destroys all infrastructure including the database. Data will be lost.

---

## Infrastructure Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                         AWS VPC                                 │
│                                                                 │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │                    EKS Cluster                           │   │
│  │                                                          │   │
│  │  ┌─────────────┐  ┌──────────────┐  ┌───────────────┐   │   │
│  │  │  Nuxt SSR   │  │ Rust Backend │  │    Redis      │   │   │
│  │  │  Frontend   │  │   (Axum)     │  │  (Bitnami)    │   │   │
│  │  └─────────────┘  └──────┬───────┘  └───────────────┘   │   │
│  │                          │                               │   │
│  │  ┌───────────────────────┼─────────────────────────┐     │   │
│  │  │ Karpenter │ ALB Ctrl │ ESO │ EBS/EFS CSI │ CNI  │     │   │
│  │  └───────────────────────┼─────────────────────────┘     │   │
│  └──────────────────────────┼───────────────────────────────┘   │
│                             │                                   │
│  ┌──────────────────────────┼───────────────────────────────┐   │
│  │           Aurora PostgreSQL  │  Secrets Manager  │  WAF  │   │
│  └──────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

### Key Infrastructure Decisions

| Component | Choice | Why |
|-----------|--------|-----|
| Compute | EKS + Karpenter (Graviton ARM64) | Cost-efficient auto-scaling |
| Database | Aurora PostgreSQL Serverless v2 | Auto-scaling, HA, managed |
| Frontend | Nuxt SSR on EKS | SSR for SEO + API proxy |
| Secrets | Secrets Manager + ESO | Zero secrets in code/env |
| Load Balancer | AWS ALB | Native K8s Ingress integration |
| WAF | AWS WAF v2 | L7 protection for ALB |

### Terraform Modules

```
iac/
├── main.tf              # Root module
├── variables.tf         # Input variables
├── terraform.tfvars     # Environment values
├── outputs.tf           # Exported values
└── modules/
    ├── vpc/             # VPC, subnets, NAT, security groups
    ├── eks-karpenter/   # EKS cluster + Karpenter auto-scaler
    ├── rds-aurora-postgresql/  # Aurora PostgreSQL cluster
    ├── eks-addons/      # IAM roles, secrets, pod identities
    ├── cognito/         # User pool for OAuth
    ├── efs/             # EFS for shared workspace storage
    └── org-cross-account/  # AWS Organizations integration
```

---

## Code Quality

Pre-commit hooks enforce quality on every commit:

```bash
uv sync                            # Setup (one-time)
uv run pre-commit run --all-files  # Run all checks
```

| Hook | Scope | Description |
|------|-------|-------------|
| cargo fmt | `backend/**/*.rs` | Rust formatting |
| cargo clippy | `backend/**/*.rs` | Rust linter (warnings = errors) |
| ESLint | `frontend/**/*.{vue,ts,js}` | TypeScript/Vue linting |
| terraform_fmt | `iac/**/*.tf` | Terraform formatting |
| terraform_validate | `iac/**/*.tf` | Terraform validation |
| detect-secrets | All | ML-based secret detection |
| aws-secrets-check | All | AWS credential pattern matching |
