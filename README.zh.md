<p align="center">
  <img src="assets/logo-full.png" alt="Ops" width="480" />
</p>

<p align="center">
  <strong>AI 驱动的自动化 SRE 平台</strong><br/>
  把 Claude agent 放进运维闭环 — 直连你的集群、遥测和发布。
</p>

<p align="center">
  <a href="README.md">English</a> · <strong>中文</strong>
</p>

---

## Ops 是什么？

Ops 是一个全栈基础设施运维平台，把 AI agent 放在 SRE 工作流的中心。你的团队不再需要在 Grafana 标签页、kubectl 窗口和 runbook 之间反复切换——直接和一个 Claude 驱动的 agent 对话，它实时访问你的多云资产清单、可观测性栈和发布流水线。

这不是一个"chat 套壳"。Agent 会对着你的真实环境执行真实工具——`kubectl`、AWS SDK、PromQL、LogQL、Argo Rollouts API——边执行边把推理过程流式输出，破坏性操作走审批闸门。

```
用户  » 生产 API 的 p99 在过去 10 分钟暴涨了 3 倍。

agent → [查询 Mimir 的延迟直方图]
     → [在热点时间窗口 grep Loki 找上游错误]
     → [查看 api-gateway 当前的 rollout]
     → [关联到 12 分钟前的那次 commit]
     → 流式输出 RCA 报告，每一条证据都带链接
```

## 核心能力

### 带完整基础设施上下文的 AI Agent

每一次会话启动时，agent 会拿到一份动态生成的上下文文件，里面包含：

- 你接入的每一个云账号、每一个集群、每一个 endpoint
- 可观测性 endpoint（Grafana/Mimir/Loki/Tempo 或 CloudWatch/X-Ray）及其认证
- 你组织内部的术语表（所以 "CAM" 就是你们定义的那个 CAM）
- Jira 和通知渠道配置，用于自动建单

能力扩展走两条路：**MCP servers**（stdio / SSE / HTTP 三种传输）和 可安装 **Skills**（底层就是真实的 `npx skills` CLI）——安全扫描、账单分析、浏览器自动化，随便你们团队建什么工具。

### 多云可观测性

信号按 runtime 自动路由：

| 信号类型 | EKS 工作负载 | AWS 服务 |
|---|---|---|
| 指标 | Mimir（PromQL） | CloudWatch |
| 日志 | Loki（LogQL） | CloudWatch Logs |
| 链路 | Tempo | X-Ray |

告警来源支持 **Grafana**、**Datadog**、**Dynatrace** webhook——统一入 issue tracker，按 fingerprint 自动去重。

### 渐进式发布（Argo Rollouts）

- **Canary** — 按步流量切换，带 analysis template
- **蓝绿** — 零停机切换，可瞬间回滚
- **Promote / Rollback** — 在 UI 里点，或者直接让 agent 操作
- **实时拓扑** — Ingress → Service → Rollout 图（vue-flow 渲染），状态实时刷新

### 根因分析（RCA）

- 任何来源的告警 webhook → 统一 issue 生命周期
- AI 驱动的 RCA，**流式输出**——你直接看着 agent 查指标、拉日志、取链路、组织证据
- 任何 issue 一键建 Jira

### 安全扫描

集成 AWS Service Screener（全球 + 中国区），持续评估安全态势：

- 多区域、多服务扫描，按账号隔离
- 发现项按严重度排序，按分类聚合（安全 / 成本 / 可靠性 / 性能）
- 支持导出 XLSX、CSV、JSON、PDF
- 每一条发现里带 AI 辅助的修复建议

## 平台特性

| 特性 | 说明 |
|---|---|
| **多租户** | 账号、用户、skills、MCP 服务、LLM provider 全部按租户隔离 |
| **RBAC** | Super Admin / Tenant Admin / User 三级，账号级读/写授权 |
| **Skills 体系** | 基于 Git 的发现与安装；按用户 workspace 符号链接隔离；会话级沙盒 |
| **MCP 服务** | stdio / SSE / HTTP 三种传输；工具级启停；内置工具自动授权 |
| **LLM Provider** | 直连 Claude、AWS Bedrock、或 Gateway endpoint，租户级配置 |
| **知识库** | Markdown 文档，Mermaid 图渲染，团队共享 |
| **术语表** | 组织专属术语，自动注入 agent 上下文 |
| **通知渠道** | Slack、飞书（Lark）、Microsoft Teams、Jira |
| **OAuth 登录** | Microsoft Azure AD + AWS Cognito，同时保留本地用户名密码 |
| **审批工作流** | 破坏性操作走审批闸门 |
| **定时任务** | Cron 驱动，带执行历史 |
| **国际化** | 中文 + 英文（默认 `zh`） |
| **主题** | 深色优先，AWS Q Developer 风格，底部 aurora 光晕标识 |

## 技术栈

| 层 | 技术 |
|---|---|
| 前端 | Nuxt 3 · Vue 3（Composition API）· TypeScript strict |
| | shadcn-vue · Radix Vue · Reka UI · Tailwind CSS · lucide icons |
| | vue-flow（拓扑）· Mermaid · Pinia · `@nuxtjs/i18n` |
| 后端 | Rust（edition 2024）· Axum 0.8 · SQLx 0.8 · Tokio |
| | `kube` 0.98 · `k8s-openapi` v1_32 · AWS SDK v1 · `reqwest` 0.12 |
| 数据库 | 本地 PostgreSQL 16 · 生产 Aurora PostgreSQL |
| AI | Claude Code CLI（子进程）· MCP · SSE 流式输出 |
| 基础设施 | Terraform · EKS + Karpenter（Graviton）· Aurora · WAF · ALB Ingress · CloudFront · Cognito |
| 密钥 | AWS Secrets Manager + External Secrets Operator |
| 发布 | Argo Rollouts · Helm · CodeBuild pipeline |

## 项目结构

```
opsagent/
├── frontend/           # Nuxt 3 SSR 应用（ops-frontend）
├── backend/            # Rust API 服务（crate 名称 "ops"）
├── iac/                # Terraform — VPC、EKS、RDS、WAF、Cognito、EFS、CloudFront
├── k8s/                # 应用 manifest + Helm 基础设施 chart
├── scripts/            # deploy-all / local-dev / build / destroy / create-user / tests
├── e2e/                # Playwright — chromium / firefox / webkit + 无障碍
├── demo/               # 演示视频、mock 微服务、解说资产
├── docs/               # 部署指南、本地开发指南、博客
├── aidlc-docs/         # AI-DLC 设计文档（按日期归档）
├── docker-compose.yml  # 本地：Postgres 16 + Redis 7
├── Dockerfile.backend  # 多阶段 Rust 构建
├── Dockerfile.frontend # 多阶段 Nuxt 构建
├── Makefile            # 测试矩阵 + 覆盖率
└── buildspec.yml       # AWS CodeBuild 流水线
```

## 快速开始

```bash
# 一条命令拉起 Postgres + Rust 后端 + Nuxt 前端
./scripts/local-dev.sh

# 打开 http://localhost:3003
# 登录：admin / admin123
```

需要 Docker、Rust（cargo）、Node ≥ 20。首次运行会自动把 `.env.example` 复制为 `.env` 并注入开发数据。

完整步骤（前置依赖、IDE 配置、排障）见 **[docs/local-development.md](docs/local-development.md)**。

## 测试

```bash
make test-all          # 全跑：后端 + 前端 + e2e + 安全 + iac
make test-backend      # cargo test
make test-frontend     # vitest
make test-e2e          # Playwright
make test-security     # cargo-audit + deny + npm-audit + trivy + gitleaks + semgrep
make test-iac          # tflint + checkov + kubeconform + kube-score + conftest
make coverage          # tarpaulin（Rust）+ V8（Vitest）HTML 覆盖率报告
```

## 部署

一条命令在 AWS 拉起完整环境：

```bash
./scripts/deploy-all.sh              # Terraform → Helm → Docker → Kubernetes
./scripts/deploy-all.sh --step 1     # 只跑某一阶段（1 = terraform，……）
./scripts/deploy-to-existing.sh      # 跳过 terraform，把应用推到现有集群
./scripts/destroy.sh                 # 全部销毁
```

分步细节见 **[docs/deployment.zh.md](docs/deployment.zh.md)**。

## 环境变量

常用环境变量（完整列表在 `.env.example`）：

| 变量 | 默认值 | 用途 |
|---|---|---|
| `OPS_ENV` | `local` | `local` · `dev` · `prod` |
| `DATABASE_URL` | `postgresql://ops:ops_dev@localhost:5432/ops` | Postgres DSN |
| `JWT_SECRET` | dev 默认值 | HS256 签名密钥——生产必须 ≥ 32 字符 |
| `ALLOWED_ORIGINS` | `http://localhost:3003` | CORS 白名单 |
| `CLAUDE_MODEL` / `CLAUDE_TIMEOUT_MS` / `CLAUDE_WORK_DIR` | `opus` / 5 分钟 / `./workspace` | Agent 子进程配置 |
| `AWS_PROFILE` / `AWS_REGION` | `default` / `us-east-1` | 本地开发用 profile，生产用 IAM role |
| `ENABLE_TUNNEL` | `false` | `local-dev.sh` 会启动 `cloudflared`，把后端暴露给 Grafana webhook |

## 设计语言

Ops 的视觉语言参考 **AWS Q Developer / Bedrock Agent Studio**：冷黑深色画布、无边框面板、AWS 蓝为主色，再加一条标志性的 **aurora 光晕**（粉紫 → 青绿 → 靛紫）从每一页底部向上晕染，作为平台"生成式 AI"的视觉暗号。组件基于 shadcn-vue + Radix Vue + Reka UI，尺寸紧凑、数据优先（`h-8` 控件、`text-xs` 正文）。

## 文档

- [本地开发](docs/local-development.md) — 完整安装、IDE、排障
- [部署](docs/deployment.md) — Terraform + Helm + Kubernetes 分步指南
- [博客：Claude Code as SRE Runtime](docs/blog/claude-code-as-sre-runtime.md)
- [AI-DLC 设计文档](aidlc-docs/) — 重要特性的设计文档（按日期归档）

## License

MIT
