# Building an AI-Powered Infrastructure Query Bot with Amazon Bedrock and Claude

*A practical guide to building a natural-language operations assistant that queries across 50+ AWS accounts in seconds.*

## The Problem

If you manage more than a handful of AWS accounts -- and most enterprises manage 10, 20, or 50+ -- you know the pain. Someone on Slack asks: "Which account owns the IP 54.200.x.x?" or "What ACM certificates expire this month?" or "What's the egress IP for the staging EKS cluster?" Each question sounds simple. Answering it is not.

The traditional workflow looks something like this: log in to the AWS Console, switch to each account one by one, navigate to the relevant service, run a query, copy the result into a spreadsheet, and repeat. If you're more advanced, you might have a collection of shell scripts or a CMDB that was accurate six months ago. Either way, the average time to answer a single cross-account infrastructure question is 15-30 minutes -- and that's if you know where to look.

This is a terrible use of an SRE's time. The information is all there in the AWS APIs. What's missing is a way to ask for it in plain language and get a structured answer back, across all accounts, in seconds. That's why we built OpsAgent.

## The Solution: OpsAgent

OpsAgent is a chatbot that lets your team ask infrastructure questions in natural language -- from Microsoft Teams, Slack, or Feishu (Lark) -- and get accurate, cross-account answers powered by Claude on Amazon Bedrock. Ask "list all EKS clusters running v1.29" and OpsAgent will scan every account in your AWS Organization, assume a read-only role in each one, query the EKS API, and return a formatted Markdown table. All within a single chat message.

The key insight behind OpsAgent is that Claude Code CLI already knows how to run `aws` CLI commands, `kubectl`, `jq`, and other standard tools. Instead of building custom MCP tools for every query scenario, we give Claude a knowledge base that explains our account structure and query patterns, a set of helper scripts for cross-account traversal, and let it reason about how to answer each question. This means zero custom tool development for new query types -- Claude figures out the right AWS CLI commands on its own.

OpsAgent also supports a plugin architecture based on the Model Context Protocol (MCP). Need to search Confluence for architecture docs? Enable the Confluence plugin in a YAML config file. Want to query Jira tickets or GitHub PRs? Add the corresponding plugin -- no code changes required. The system is designed so that adding a new data source is a config change, not a development project.

## Architecture Deep Dive

```
                          IM Platforms
               (Teams / Slack / Feishu)
                           |
                           v
                   +---------------+
                   |      ALB      |
                   +-------+-------+
                           |
              +------------+------------+
              |                         |
              v                         v
     /api/messages/*              /admin/*
              |                         |
              v                         v
   +-------------------+     +-------------------+
   |   ECS Fargate     |     |   Admin API       |
   |   (4 vCPU, 8 GB)  |     |   Config CRUD     |
   |                   |     |   Knowledge Base   |
   |   Node.js Server  |     +-------------------+
   |        |          |
   |        v          |            +-------+
   |  Claude Code CLI  |            |  EFS  |
   |  (Bedrock Opus)   |----------->| Knowledge
   |        |          |            | Base  |
   +--------+----------+            +-------+
            |
            |  aws organizations list-accounts
            |  sts:AssumeRole per account
            |
   +--------+--------+--------+--------+
   |        |        |        |        |
   v        v        v        v        v
 Acct A   Acct B   Acct C   Acct D   ...
 (ReadOnly Role in each member account)
```

The flow works as follows:

1. A user sends a question in Teams, Slack, or Feishu.
2. The platform webhook hits an Application Load Balancer, which routes to an ECS Fargate service.
3. The Node.js server identifies the platform, parses the message, and passes it to the Claude Code CLI in non-interactive mode.
4. Claude Code (running Claude Opus on Amazon Bedrock) reads the knowledge base, decides which AWS CLI commands to run, and executes them -- including cross-account AssumeRole calls via AWS Organizations auto-discovery.
5. The structured result is sent back through the same platform adapter.

For cross-account access, OpsAgent uses AWS Organizations to automatically discover all member accounts. A CloudFormation StackSet deploys an `OpsAgentReadOnly` IAM role to every member account. The ECS task role in the hub account has permission to assume this role in any member account. When a new account is added to the Organization, the StackSet automatically deploys the role -- zero configuration needed.

The knowledge base lives on an EFS volume mounted to the Fargate task. It contains company-specific context: glossary terms, account naming conventions, common query patterns, and runbooks. An admin API (protected by API key authentication) allows the team to update knowledge base files, glossary, and account configurations through a web UI without redeploying.

## Key Design Decisions

**Why Claude Code CLI instead of custom MCP tools?** Claude Code CLI already ships with Bash, Read, Glob, and Grep tools. That means it can run any `aws` CLI command, parse JSON with `jq`, execute `kubectl`, and read files -- all out of the box. Instead of building and maintaining 18+ custom MCP tools (one for each query type), we write a knowledge base file that teaches Claude *how* to query our infrastructure. When a new query pattern emerges, we add a paragraph to the knowledge base, not a new code module.

**Why ECS Fargate over Lambda?** Complex cross-account queries can take 2-5 minutes -- scanning 50 accounts for EKS clusters across multiple regions involves hundreds of API calls. Lambda's 15-minute timeout would technically work, but the cold start overhead, the need for a container image with `aws` CLI, `kubectl`, and `jq` pre-installed, and the inability to maintain conversation sessions make Fargate a better fit. The 4 vCPU / 8 GB configuration handles concurrent queries well.

**Why knowledge base files over fine-tuning?** Knowledge base files on EFS can be updated through the admin API in seconds, with no model retraining or redeployment required. They're plain text or Markdown, easy to version control, and easy for anyone on the team to edit. The glossary system injects company terminology directly into the system prompt, so Claude understands that "ics-stg-aps1" means "the ICS staging cluster in ap-southeast-1."

**Plugin architecture: add data sources with config, not code.** The `config/plugins.yaml` file defines MCP server plugins. Each plugin specifies a command, arguments, and environment variables. To add Confluence search, you enable the plugin and provide a token. The plugin loader reads this YAML at query time and generates the MCP configuration dynamically. The same pattern works for Jira, GitHub, or any MCP-compatible server.

```yaml
plugins:
  confluence:
    enabled: true
    type: mcp
    command: "npx"
    args: ["-y", "@anthropic-ai/mcp-remote", "https://mcp.atlassian.com/v1/sse"]
    env:
      MCP_HEADERS: "Authorization: Bearer ${CONFLUENCE_TOKEN}"
```

## Real-World Examples

**Example 1: Find which account owns an IP address**

> **User:** Which account owns the IP 54.200.12.34?
>
> **OpsAgent:** Scanning all 42 accounts in the Organization...
>
> | Account | Account ID | Region | Resource Type | Resource |
> |---------|-----------|--------|---------------|----------|
> | prod-us-west | 111122223333 | us-west-2 | Elastic IP | eipalloc-0abc123 (attached to i-0def456) |
>
> The IP 54.200.12.34 belongs to the **prod-us-west** account, attached to EC2 instance `i-0def456` in us-west-2.

**Example 2: Cross-account EKS cluster inventory**

> **User:** List all EKS clusters running Kubernetes v1.29 or older.
>
> **OpsAgent:**
>
> | Account | Account ID | Region | Version | Cluster | Status |
> |---------|-----------|--------|---------|---------|--------|
> | dev-aps1 | 111122223333 | ap-southeast-1 | v1.28 | dev-cluster | ACTIVE |
> | staging-use1 | 444455556666 | us-east-1 | v1.29 | stg-main | ACTIVE |
> | prod-euw1 | 777788889999 | eu-west-1 | v1.29 | prod-eu | ACTIVE |
>
> Found **3 clusters** on v1.29 or older across 42 accounts. Consider upgrading dev-cluster (v1.28) first -- it's 2 minor versions behind the latest.

**Example 3: Proactive certificate expiration check**

> **User:** What ACM certificates expire this month?
>
> **OpsAgent:**
>
> | Account | Region | Domain | Expires | Days Left | Used By |
> |---------|--------|--------|---------|-----------|---------|
> | prod-us | us-east-1 | api.example.com | 2026-03-21 | 12 | CloudFront d-abc123 |
> | prod-ap | ap-southeast-1 | app.example.com | 2026-03-28 | 19 | ALB arn:aws:...prod-alb |
>
> **2 certificates** expire this month. The api.example.com cert has only 12 days remaining -- recommend renewing immediately.

**Example 4: Search architecture documentation**

> **User:** What's the architecture for the payment service?
>
> **OpsAgent:** Searching Confluence for payment service architecture...
>
> Found the following documentation:
> - **Payment Service Architecture** (last updated: 2026-02-15) - [View in Confluence](https://wiki.example.com/pages/12345)
>   - Runs on EKS cluster `prod-aps1` in ap-southeast-1
>   - PostgreSQL RDS (db.r6g.xlarge) with read replicas
>   - Redis ElastiCache for session management
>   - Processes ~50k transactions/day

## Security Considerations

OpsAgent is designed with a security-first approach:

**Read-only access only.** The `OpsAgentReadOnly` role in each member account is attached to the AWS-managed `ReadOnlyAccess` policy. It cannot create, modify, or delete any resources. The Claude Code CLI is configured with `--allowedTools 'Bash(git*:deny),Read,Glob,Grep'` -- no write tools are permitted.

**IAM role per member account via CloudFormation StackSets.** The `MemberRoleStack` uses a service-managed StackSet that automatically deploys to all accounts in the Organization. The trust policy restricts AssumeRole to the specific ECS task role ARN in the hub account, with an external ID condition for additional protection.

```typescript
Condition: {
  StringEquals: {
    'sts:ExternalId': 'opsagent',
  },
},
```

**Secrets Manager for all credentials.** IM platform tokens (Teams App Password, Slack Bot Token, Feishu App Secret) and plugin credentials (Confluence Token) are stored in AWS Secrets Manager under `opsagent/bot-secrets`. They are injected into the ECS task as environment variables at runtime -- never baked into the container image.

**Audit logging for all queries.** Every query is logged with the user identity, platform, timestamp, query text, response length, duration, and success/failure status. Logs go to CloudWatch Logs for centralized monitoring and alerting.

**Admin API authentication.** The admin API requires an `X-Admin-Key` header for all configuration and knowledge base operations. CORS is configurable to restrict the admin UI to specific origins.

## Getting Started

**Prerequisites:**
- An AWS Organization with member accounts
- Amazon Bedrock access with the Claude Opus model enabled in your hub account
- AWS CDK v2 and Node.js >= 20 installed
- At least one IM platform configured (Teams, Slack, or Feishu)

**Deployment in 3 steps:**

```bash
# Step 1: Deploy the main stack (VPC, ECS, ALB, EFS, IAM, ECR, CodeBuild)
cd infra && npx cdk deploy OpsAgentStack --require-approval never

# Step 2: Deploy ReadOnly roles to all member accounts
npx cdk deploy MemberRoleStack -c organizationRootOuId=r-xxxx --require-approval never

# Step 3: Configure IM platform secrets in Secrets Manager
# (Teams: MICROSOFT_APP_ID + MICROSOFT_APP_PASSWORD)
# (Slack: SLACK_BOT_TOKEN + SLACK_SIGNING_SECRET)
# (Feishu: FEISHU_APP_ID + FEISHU_APP_SECRET + FEISHU_VERIFICATION_TOKEN)
```

After deployment, build and push the container image via CodeBuild, then trigger a rolling update on ECS. Point your IM platform's webhook URL to the ALB endpoint and start asking questions.

## What's Next

OpsAgent is already useful for day-to-day operations, but there's more on the roadmap:

- **Cost optimization with model routing.** Use Claude Haiku for simple lookups ("what's the IP of X?") and reserve Opus for complex multi-account analysis. This can reduce Bedrock costs by 60-80% for typical usage patterns.
- **Streaming responses for better UX.** Instead of waiting 2-3 minutes for a complete answer, stream intermediate results as Claude discovers them. "Scanning account 12 of 42... found 3 clusters so far."
- **RBAC for team-level access control.** Different teams should see different accounts. A payments team doesn't need access to marketing infrastructure, and vice versa.
- **More plugins.** Datadog for metrics and alerts, PagerDuty for on-call schedules and incident context, ServiceNow for change management records. Each one is a YAML config entry away.
- **Write operations with approval workflows.** Carefully scoped actions like "restart this ECS service" or "scale this ASG to 5" with a human-in-the-loop approval step before execution.

The core idea remains the same: let the AI agent use the tools that already exist (AWS CLI, kubectl, MCP plugins) and teach it your organization's context through knowledge base files. No custom tool development. No CMDB to maintain. Just ask the question and get the answer.

---

# 用 Amazon Bedrock 和 Claude 构建 AI 基础设施查询机器人

*一份实用指南：构建一个自然语言运维助手，在数秒内跨 50+ AWS 账号查询基础设施信息。*

## 问题所在

如果你管理的 AWS 账号超过几个 -- 大多数企业管理着 10 个、20 个甚至 50 个以上 -- 你一定体会过这种痛苦。有人在 Slack 上问："IP 54.200.x.x 属于哪个账号？"或者"这个月有哪些 ACM 证书要过期？"又或者"测试环境 EKS 集群的出口 IP 是什么？"每个问题听起来都很简单，回答起来却完全不是那么回事。

传统的工作流程大概是这样的：登录 AWS Console，逐个切换账号，导航到相关服务，执行查询，把结果复制到表格里，然后重复以上步骤。如果你更进阶一些，可能有一组 Shell 脚本或者一个六个月前还算准确的 CMDB。无论哪种方式，回答一个跨账号的基础设施问题平均需要 15-30 分钟 -- 前提是你知道该去哪里找。

这对 SRE 的时间来说是极大的浪费。所有信息都在 AWS API 里，缺少的只是一种用自然语言提问、跨所有账号在数秒内获得结构化答案的方式。这就是我们构建 OpsAgent 的原因。

## 解决方案：OpsAgent

OpsAgent 是一个聊天机器人，让你的团队能够通过 Microsoft Teams、Slack 或飞书用自然语言提问基础设施问题，并获得由 Amazon Bedrock 上的 Claude 驱动的准确、跨账号的回答。问一句 "列出所有运行 v1.29 的 EKS 集群"，OpsAgent 就会扫描 AWS Organization 中的每个账号，在每个账号中假设一个只读角色，查询 EKS API，并返回格式化的 Markdown 表格。所有这些都在一条聊天消息内完成。

OpsAgent 背后的核心思路是：Claude Code CLI 已经知道如何运行 `aws` CLI 命令、`kubectl`、`jq` 和其他标准工具。与其为每种查询场景构建自定义 MCP 工具，不如给 Claude 一个知识库来解释我们的账号结构和查询模式，再加上一组跨账号遍历的辅助脚本，让它自己推理如何回答每个问题。这意味着新的查询类型无需自定义工具开发 -- Claude 自己就能推算出正确的 AWS CLI 命令。

OpsAgent 还支持基于 Model Context Protocol (MCP) 的插件架构。需要搜索 Confluence 上的架构文档？在 YAML 配置文件中启用 Confluence 插件即可。想查询 Jira 工单或 GitHub PR？添加对应的插件 -- 无需改动代码。系统的设计目标是：添加新数据源只是一次配置变更，而不是一个开发项目。

## 架构详解

```
                          即时通讯平台
               (Teams / Slack / 飞书)
                           |
                           v
                   +---------------+
                   |      ALB      |
                   +-------+-------+
                           |
              +------------+------------+
              |                         |
              v                         v
     /api/messages/*              /admin/*
              |                         |
              v                         v
   +-------------------+     +-------------------+
   |   ECS Fargate     |     |   管理 API         |
   |   (4 vCPU, 8 GB)  |     |   配置 CRUD        |
   |                   |     |   知识库管理         |
   |   Node.js 服务    |     +-------------------+
   |        |          |
   |        v          |            +-------+
   |  Claude Code CLI  |            |  EFS  |
   |  (Bedrock Opus)   |----------->| 知识库 |
   |        |          |            |       |
   +--------+----------+            +-------+
            |
            |  aws organizations list-accounts
            |  sts:AssumeRole 逐账号访问
            |
   +--------+--------+--------+--------+
   |        |        |        |        |
   v        v        v        v        v
 账号 A   账号 B   账号 C   账号 D   ...
 (每个成员账号中部署 ReadOnly Role)
```

整体流程如下：

1. 用户在 Teams、Slack 或飞书中发送一个问题。
2. 平台 Webhook 请求到达 Application Load Balancer，路由到 ECS Fargate 服务。
3. Node.js 服务识别平台类型、解析消息，并以非交互模式将其传递给 Claude Code CLI。
4. Claude Code（在 Amazon Bedrock 上运行 Claude Opus 模型）读取知识库，决定需要执行哪些 AWS CLI 命令，并执行这些命令 -- 包括通过 AWS Organizations 自动发现进行跨账号 AssumeRole 调用。
5. 结构化的结果通过相同的平台适配器发送回用户。

在跨账号访问方面，OpsAgent 使用 AWS Organizations 自动发现所有成员账号。通过 CloudFormation StackSet 在每个成员账号中部署一个 `OpsAgentReadOnly` IAM 角色。Hub 账号中的 ECS 任务角色有权限在任何成员账号中假设该角色。当新账号加入 Organization 时，StackSet 会自动部署角色 -- 无需任何配置。

知识库存储在挂载到 Fargate 任务的 EFS 卷上，包含公司特定的上下文信息：术语表、账号命名规范、常见查询模式和运维手册。管理 API（通过 API Key 认证保护）允许团队通过 Web UI 更新知识库文件、术语表和账号配置，无需重新部署。

## 关键设计决策

**为什么选择 Claude Code CLI 而不是自定义 MCP 工具？** Claude Code CLI 自带 Bash、Read、Glob 和 Grep 工具。这意味着它可以开箱即用地运行任何 `aws` CLI 命令、用 `jq` 解析 JSON、执行 `kubectl`、读取文件。与其构建和维护 18 个以上的自定义 MCP 工具（每种查询类型一个），不如编写一个知识库文件来教 Claude *如何*查询我们的基础设施。当出现新的查询模式时，我们在知识库里加一段话，而不是增加一个新的代码模块。

**为什么选择 ECS Fargate 而不是 Lambda？** 复杂的跨账号查询可能需要 2-5 分钟 -- 扫描 50 个账号中多个区域的 EKS 集群涉及数百次 API 调用。Lambda 的 15 分钟超时在理论上可以工作，但冷启动开销、需要包含 `aws` CLI、`kubectl` 和 `jq` 的容器镜像，以及无法维护对话会话等因素，使得 Fargate 更为合适。4 vCPU / 8 GB 的配置能够很好地处理并发查询。

**为什么选择知识库文件而不是模型微调？** EFS 上的知识库文件可以通过管理 API 在数秒内更新，无需模型重训或重新部署。它们是纯文本或 Markdown 格式，易于版本控制，团队中任何人都可以编辑。术语表系统将公司术语直接注入系统提示词，让 Claude 理解 "ics-stg-aps1" 的意思是 "ap-southeast-1 区域的 ICS 测试环境集群"。

**插件架构：通过配置而非代码添加数据源。** `config/plugins.yaml` 文件定义了 MCP 服务器插件。每个插件指定了命令、参数和环境变量。要添加 Confluence 搜索，只需启用插件并提供令牌。插件加载器在查询时读取该 YAML 文件并动态生成 MCP 配置。同样的模式适用于 Jira、GitHub 或任何 MCP 兼容的服务器。

```yaml
plugins:
  confluence:
    enabled: true
    type: mcp
    command: "npx"
    args: ["-y", "@anthropic-ai/mcp-remote", "https://mcp.atlassian.com/v1/sse"]
    env:
      MCP_HEADERS: "Authorization: Bearer ${CONFLUENCE_TOKEN}"
```

## 真实使用示例

**示例 1：查找 IP 地址所属账号**

> **用户:** 54.200.12.34 这个 IP 属于哪个账号？
>
> **OpsAgent:** 正在扫描 Organization 中的全部 42 个账号...
>
> | 账号 | 账号 ID | 区域 | 资源类型 | 资源 |
> |------|---------|------|----------|------|
> | prod-us-west | 111122223333 | us-west-2 | 弹性 IP | eipalloc-0abc123 (绑定到 i-0def456) |
>
> IP 54.200.12.34 属于 **prod-us-west** 账号，绑定在 us-west-2 区域的 EC2 实例 `i-0def456` 上。

**示例 2：跨账号 EKS 集群清单**

> **用户:** 列出所有运行 Kubernetes v1.29 或更早版本的 EKS 集群。
>
> **OpsAgent:**
>
> | 账号 | 账号 ID | 区域 | 版本 | 集群 | 状态 |
> |------|---------|------|------|------|------|
> | dev-aps1 | 111122223333 | ap-southeast-1 | v1.28 | dev-cluster | ACTIVE |
> | staging-use1 | 444455556666 | us-east-1 | v1.29 | stg-main | ACTIVE |
> | prod-euw1 | 777788889999 | eu-west-1 | v1.29 | prod-eu | ACTIVE |
>
> 在 42 个账号中发现 **3 个集群**运行 v1.29 或更早版本。建议优先升级 dev-cluster (v1.28) -- 它落后最新版本 2 个小版本。

**示例 3：主动检查证书过期**

> **用户:** 这个月有哪些 ACM 证书要过期？
>
> **OpsAgent:**
>
> | 账号 | 区域 | 域名 | 过期时间 | 剩余天数 | 使用方 |
> |------|------|------|----------|----------|--------|
> | prod-us | us-east-1 | api.example.com | 2026-03-21 | 12 | CloudFront d-abc123 |
> | prod-ap | ap-southeast-1 | app.example.com | 2026-03-28 | 19 | ALB arn:aws:...prod-alb |
>
> 本月有 **2 个证书**到期。api.example.com 的证书仅剩 12 天 -- 建议立即续期。

**示例 4：搜索架构文档**

> **用户:** 支付服务的架构是什么？
>
> **OpsAgent:** 正在 Confluence 中搜索支付服务架构...
>
> 找到以下文档：
> - **支付服务架构** (最后更新: 2026-02-15) - [在 Confluence 中查看](https://wiki.example.com/pages/12345)
>   - 运行在 ap-southeast-1 区域的 EKS 集群 `prod-aps1` 上
>   - PostgreSQL RDS (db.r6g.xlarge)，配有只读副本
>   - Redis ElastiCache 用于会话管理
>   - 日均处理约 5 万笔交易

## 安全考量

OpsAgent 以安全优先的理念设计：

**仅限只读访问。** 每个成员账号中的 `OpsAgentReadOnly` 角色绑定了 AWS 托管策略 `ReadOnlyAccess`，不能创建、修改或删除任何资源。Claude Code CLI 配置为 `--allowedTools 'Bash(git*:deny),Read,Glob,Grep'` -- 不允许任何写操作工具。

**通过 CloudFormation StackSets 在每个成员账号部署 IAM 角色。** `MemberRoleStack` 使用服务托管的 StackSet，自动部署到 Organization 中的所有账号。信任策略将 AssumeRole 限制为 Hub 账号中特定的 ECS 任务角色 ARN，并附加了外部 ID 条件作为额外保护。

```typescript
Condition: {
  StringEquals: {
    'sts:ExternalId': 'opsagent',
  },
},
```

**所有凭证存储在 Secrets Manager 中。** 即时通讯平台令牌（Teams App Password、Slack Bot Token、飞书 App Secret）和插件凭证（Confluence Token）存储在 AWS Secrets Manager 的 `opsagent/bot-secrets` 中，在运行时作为环境变量注入 ECS 任务 -- 绝不会打包到容器镜像中。

**所有查询的审计日志。** 每次查询都会记录用户身份、平台、时间戳、查询文本、响应长度、耗时和成功/失败状态。日志发送到 CloudWatch Logs，用于集中监控和告警。

**管理 API 认证。** 管理 API 的所有配置和知识库操作都需要 `X-Admin-Key` 请求头。CORS 可配置，以限制管理 UI 只能从特定来源访问。

## 快速开始

**前置条件：**
- 包含成员账号的 AWS Organization
- Hub 账号中启用 Amazon Bedrock 的 Claude Opus 模型访问
- 已安装 AWS CDK v2 和 Node.js >= 20
- 至少配置一个即时通讯平台（Teams、Slack 或飞书）

**3 步部署：**

```bash
# 第 1 步：部署主栈（VPC、ECS、ALB、EFS、IAM、ECR、CodeBuild）
cd infra && npx cdk deploy OpsAgentStack --require-approval never

# 第 2 步：在所有成员账号部署 ReadOnly 角色
npx cdk deploy MemberRoleStack -c organizationRootOuId=r-xxxx --require-approval never

# 第 3 步：在 Secrets Manager 中配置即时通讯平台密钥
# (Teams: MICROSOFT_APP_ID + MICROSOFT_APP_PASSWORD)
# (Slack: SLACK_BOT_TOKEN + SLACK_SIGNING_SECRET)
# (飞书: FEISHU_APP_ID + FEISHU_APP_SECRET + FEISHU_VERIFICATION_TOKEN)
```

部署完成后，通过 CodeBuild 构建并推送容器镜像，然后触发 ECS 滚动更新。将即时通讯平台的 Webhook URL 指向 ALB 端点，就可以开始提问了。

## 未来规划

OpsAgent 已经能够满足日常运维需求，但路线图上还有更多计划：

- **通过模型路由优化成本。** 简单查询（"X 的 IP 是什么？"）使用 Claude Haiku，复杂的多账号分析保留给 Opus。这可以将典型使用场景下的 Bedrock 成本降低 60-80%。
- **流式响应提升用户体验。** 不再等待 2-3 分钟才能看到完整答案，而是在 Claude 发现结果时逐步推送中间结果。"正在扫描第 12/42 个账号... 目前已发现 3 个集群。"
- **基于角色的访问控制 (RBAC)。** 不同团队应该看到不同的账号。支付团队不需要访问市场营销的基础设施，反之亦然。
- **更多插件。** Datadog 用于指标和告警，PagerDuty 用于值班排班和事件上下文，ServiceNow 用于变更管理记录。每一个都只需要在 YAML 配置中添加一个条目。
- **带审批流程的写操作。** 精心限定范围的操作，如"重启这个 ECS 服务"或"将这个 ASG 扩展到 5 个实例"，在执行前需要人工审批。

核心理念始终如一：让 AI 代理使用已有的工具（AWS CLI、kubectl、MCP 插件），通过知识库文件教会它你的组织上下文。无需自定义工具开发，无需维护 CMDB。只需提出问题，即可获得答案。
