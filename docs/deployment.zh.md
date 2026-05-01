# 部署指南

<p align="right"><a href="deployment.md">English</a> · <strong>中文</strong></p>

## AWS 部署（EKS）

### 前置依赖

| 工具 | 版本 | 用途 |
|---|---|---|
| AWS CLI | v2 | 云操作 |
| Terraform | 1.5+ | 基础设施 provisioning |
| kubectl | 1.28+ | Kubernetes 管理 |
| Helm | 3.12+ | Chart 部署 |
| Docker | 20+ | 镜像构建 |

### 全量部署（单条命令）

```bash
./scripts/deploy-all.sh
```

脚本顺序执行以下 4 个阶段：

1. **Terraform Apply** — VPC、EKS + Karpenter、RDS Aurora、WAF、Secrets Manager、Cognito
2. **Helm Install** — ALB Controller、Karpenter、External Secrets Operator、Redis
3. **Docker 构建 & 推送** — 构建后端/前端镜像，推送到 ECR
4. **K8s 部署** — 应用 ConfigMap、Secret、Deployment、Service、Ingress

### 分阶段执行

```bash
./scripts/deploy-all.sh --step 1   # 只跑 Terraform
./scripts/deploy-all.sh --step 2   # 只跑 Helm chart
./scripts/deploy-all.sh --step 3   # 只构建推送 Docker 镜像
./scripts/deploy-all.sh --step 4   # 只部署 K8s 应用
```

### 只更新应用（跳过基础设施）

仅修改应用代码、不涉及基础设施时：

```bash
./scripts/deploy-to-existing.sh
```

这会构建新镜像、推到 ECR、在 EKS 上滚动发布。

### 只构建并推送镜像

```bash
./scripts/build-and-push.sh               # 前后端都构建
./scripts/build-and-push.sh --backend     # 只构建后端
./scripts/build-and-push.sh --frontend    # 只构建前端
```

### 销毁

```bash
./scripts/destroy.sh
```

> **警告**：会把所有基础设施连同数据库一起销毁，数据会丢失。

---

## 基础设施架构

```
┌─────────────────────────────────────────────────────────────────┐
│                         AWS VPC                                 │
│                                                                 │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │                    EKS 集群                              │   │
│  │                                                          │   │
│  │  ┌─────────────┐  ┌──────────────┐  ┌───────────────┐   │   │
│  │  │  Nuxt SSR   │  │  Rust 后端   │  │    Redis      │   │   │
│  │  │   前端       │  │   (Axum)     │  │  (Bitnami)    │   │   │
│  │  └─────────────┘  └──────┬───────┘  └───────────────┘   │   │
│  │                          │                               │   │
│  │  ┌───────────────────────┼─────────────────────────┐     │   │
│  │  │ Karpenter │ ALB Ctrl │ ESO │ EBS/EFS CSI │ CNI  │     │   │
│  │  └───────────────────────┼─────────────────────────┘     │   │
│  └──────────────────────────┼───────────────────────────────┘   │
│                             │                                   │
│  ┌──────────────────────────┼───────────────────────────────┐   │
│  │        Aurora PostgreSQL  │  Secrets Manager  │  WAF     │   │
│  └──────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

### 关键基础设施选型

| 组件 | 选型 | 原因 |
|---|---|---|
| 计算 | EKS + Karpenter（Graviton ARM64） | 成本优先的自动伸缩 |
| 数据库 | Aurora PostgreSQL Serverless v2 | 自动伸缩、高可用、托管 |
| 前端 | EKS 上的 Nuxt SSR | SSR 带 SEO + API 代理 |
| 密钥 | Secrets Manager + ESO | 代码和 env 里零密钥 |
| 负载均衡 | AWS ALB | 原生 K8s Ingress 集成 |
| WAF | AWS WAF v2 | ALB 的七层防护 |

### Terraform 模块

```
iac/
├── main.tf              # 根模块
├── variables.tf         # 输入变量
├── terraform.tfvars     # 环境值
├── outputs.tf           # 输出值
└── modules/
    ├── vpc/             # VPC、子网、NAT、安全组
    ├── eks-karpenter/   # EKS 集群 + Karpenter 伸缩器
    ├── rds-aurora-postgresql/  # Aurora PostgreSQL 集群
    ├── eks-addons/      # IAM role、secrets、pod identity
    ├── cognito/         # OAuth 用的 user pool
    ├── efs/             # 共享 workspace 存储用的 EFS
    └── org-cross-account/  # AWS Organizations 集成
```

---

## 代码质量

每次 commit 都会跑 pre-commit 钩子：

```bash
uv sync                            # 初始化（一次性）
uv run pre-commit run --all-files  # 跑全部检查
```

| 钩子 | 范围 | 说明 |
|---|---|---|
| cargo fmt | `backend/**/*.rs` | Rust 格式化 |
| cargo clippy | `backend/**/*.rs` | Rust linter（warning 即 error） |
| ESLint | `frontend/**/*.{vue,ts,js}` | TypeScript/Vue 检查 |
| terraform_fmt | `iac/**/*.tf` | Terraform 格式化 |
| terraform_validate | `iac/**/*.tf` | Terraform 校验 |
| detect-secrets | 全部 | 基于 ML 的密钥检测 |
| aws-secrets-check | 全部 | AWS 凭证模式匹配 |
