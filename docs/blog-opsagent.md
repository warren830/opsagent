# 用 Amazon Bedrock 和 Claude 构建多云 AI 运维助手

*一份实用指南：构建一个自然语言运维助手，在数秒内跨多个云平台和数十个账号查询基础设施信息。*

## 问题所在

如果你管理的云账号超过几个 -- 大多数企业管理着 10 个、20 个甚至 50 个以上，而且往往跨越 AWS、阿里云、Azure 等多个云平台 -- 你一定体会过这种痛苦。有人在 Slack 上问："IP 54.200.x.x 属于哪个账号？"或者"这个月有哪些 ACM 证书要过期？"又或者"阿里云生产环境的 ECS 实例列表？"每个问题听起来都很简单，回答起来却完全不是那么回事。

传统的工作流程大概是这样的：登录 AWS Console，逐个切换账号，导航到相关服务，执行查询，把结果复制到表格里，然后重复以上步骤。如果涉及多个云平台，还得分别登录不同的控制台。如果你更进阶一些，可能有一组 Shell 脚本或者一个六个月前还算准确的 CMDB。无论哪种方式，回答一个跨账号的基础设施问题平均需要 15-30 分钟 -- 前提是你知道该去哪里找。

这对 SRE 的时间来说是极大的浪费。所有信息都在各云平台的 API 里，缺少的只是一种用自然语言提问、跨所有账号在数秒内获得结构化答案的方式。这就是我们构建 OpsAgent 的原因。

## 解决方案：OpsAgent

OpsAgent 是一个多云聊天机器人，让你的团队能够通过 Microsoft Teams、Slack 或飞书用自然语言提问基础设施问题，并获得由 Amazon Bedrock 上的 Claude 驱动的准确、跨账号的回答。问一句 "列出所有运行 v1.29 的 EKS 集群"，OpsAgent 就会扫描 AWS Organization 中的每个账号，在每个账号中假设一个只读角色，查询 EKS API，并返回格式化的 Markdown 表格。所有这些都在一条聊天消息内完成。

OpsAgent 支持**多云管理**——除了 AWS，还可以配置阿里云、Azure、GCP 的账号。Claude 会根据查询自动选择对应的 CLI 工具（`aws`、`aliyun`、`az`、`gcloud`、`kubectl`）执行查询。

OpsAgent 背后的核心思路是：Claude Code CLI 已经知道如何运行这些 CLI 命令和标准工具。与其为每种查询场景构建自定义 MCP 工具，不如给 Claude 一个知识库来解释我们的账号结构和查询模式，再加上一套**可配置的 Skills（技能指引）**来教它特定的运维流程，让它自己推理如何回答每个问题。这意味着新的查询类型无需自定义工具开发 -- Claude 自己就能推算出正确的命令。

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
   |   ECS Fargate     |     |   Admin UI        |
   |   (4 vCPU, 8 GB)  |     |   平台集成管理     |
   |                   |     |   术语/账号/Skills  |
   |   Node.js 服务    |     |   知识库管理        |
   |        |          |     +-------------------+
   |        v          |
   |  Claude Code CLI  |            +-------+
   |  (Bedrock Opus)   |            |  EFS  |
   |        |          |----------->| 知识库 |
   |        |          |            | 配置   |
   +--------+----------+            +-------+
            |
            |  多云 CLI 查询
            |  aws / aliyun / az / gcloud / kubectl
            |
   +--------+--------+--------+--------+
   |        |        |        |        |
   v        v        v        v        v
 AWS      AWS      阿里云    Azure    GCP
 账号 A   账号 B   账号 C   订阅 D   项目 E
```

整体流程如下：

1. 用户在 Teams、Slack 或飞书中发送一个问题。
2. 平台 Webhook 请求到达 Application Load Balancer，路由到 ECS Fargate 服务。
3. Node.js 服务识别平台类型、解析消息，并以非交互模式将其传递给 Claude Code CLI。
4. Claude Code（在 Amazon Bedrock 上运行 Claude Opus 模型）读取知识库和 Skills 配置，决定需要执行哪些命令，并执行这些命令 -- 包括跨云平台、跨账号的查询。
5. 结构化的结果通过相同的平台适配器发送回用户。

在 AWS 跨账号访问方面，OpsAgent 使用 AWS Organizations 自动发现所有成员账号。通过 CloudFormation StackSet 在每个成员账号中部署一个 `OpsAgentReadOnly` IAM 角色。Hub 账号中的 ECS 任务角色有权限在任何成员账号中假设该角色。当新账号加入 Organization 时，StackSet 会自动部署角色 -- 无需任何配置。

**知识库和配置**存储在挂载到 Fargate 任务的 EFS 卷上。配置文件（平台凭证、术语表、账号、Skills）也持久化在 EFS 上，通过 Admin UI 修改后在服务重启时自动生效，不会因为容器更新而丢失。

## Admin UI：全功能管理界面

OpsAgent 提供了一个 Web 管理界面（`/admin`），包含以下功能标签页：

- **Chat** -- 直接与 OpsAgent 对话，测试查询效果
- **Glossary** -- 公司术语管理（缩写 → 全称 + 描述 + 关联账号/服务），自动注入系统提示词
- **Accounts** -- AWS 账号管理（额外账号、覆盖配置、默认角色），以及多云账号配置（阿里云、Azure、GCP）
- **Platform Integrations** -- 动态添加/移除即时通讯平台，直接填入凭证（App ID、Token、Secret），无需 Secrets Manager
- **Knowledge** -- 知识库文件管理，支持拖拽上传，Claude Code 运行时用 Grep/Read 自主搜索
- **Skills** -- 配置技能指引，每个 Skill 是一段注入系统提示词的自定义指令，可独立启用/禁用

## 关键设计决策

**为什么选择 Claude Code CLI 而不是自定义 MCP 工具？** Claude Code CLI 自带 Bash、Read、Glob 和 Grep 工具。这意味着它可以开箱即用地运行任何 CLI 命令、用 `jq` 解析 JSON、执行 `kubectl`、读取文件。与其构建和维护十几个自定义 MCP 工具，不如编写知识库文件和 Skills 来教 Claude *如何*查询我们的基础设施。当出现新的查询模式时，在 Admin UI 里加一个 Skill 或更新知识库即可，而不是增加新的代码模块。

**为什么选择 ECS Fargate 而不是 Lambda？** 复杂的跨账号查询可能需要 2-5 分钟 -- 扫描 50 个账号中多个区域的 EKS 集群涉及数百次 API 调用。Lambda 的 15 分钟超时在理论上可以工作，但冷启动开销、需要包含多个云 CLI 工具的容器镜像，以及无法维护对话会话等因素，使得 Fargate 更为合适。4 vCPU / 8 GB 的配置能够很好地处理并发查询。

**为什么选择知识库文件而不是模型微调？** EFS 上的知识库文件可以通过管理 API 在数秒内更新，无需模型重训或重新部署。它们是纯文本或 Markdown 格式，易于版本控制，团队中任何人都可以编辑。术语表系统将公司术语直接注入系统提示词，让 Claude 理解 "ics-stg-aps1" 的意思是 "ap-southeast-1 区域的 ICS 测试环境集群"。

**Skills 系统：可配置的运维指引。** 每个 Skill 是一段自定义指令，在启用时注入 Claude 的系统提示词。例如一个 "ECS 故障排查" Skill 可以教 Claude 标准的排查流程：先检查 service 状态、再看任务失败原因、再查日志。Skill 通过 Admin UI 管理，可以随时启用或禁用，无需重新部署。

```yaml
skills:
  - name: "ECS Troubleshooting"
    description: "ECS 服务故障排查流程"
    enabled: true
    instructions: |
      当用户询问 ECS 相关问题时:
      1. 先用 aws ecs describe-services 检查 service 状态
      2. 用 aws ecs list-tasks 查看任务
      3. 用 aws ecs describe-tasks 检查失败原因
      4. 用 aws logs 查看相关日志
```

**动态平台集成：通过 Admin UI 管理凭证。** 即时通讯平台不再内置固定的三个，而是通过 Admin UI 动态添加。每个平台填入对应的凭证（如 Slack 的 Bot Token + Signing Secret），保存到 EFS 上的配置文件。不再依赖 Secrets Manager —— 配置文件在 EFS 上加密存储，VPC 内网隔离保障安全。

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

## 多云支持

OpsAgent 不仅限于 AWS。通过 Admin UI 的 Accounts 页面，可以配置多个云平台的账号：

| 云平台 | CLI 工具 | 配置字段 |
|--------|---------|----------|
| AWS | `aws` | 账号 ID、角色名、区域 |
| 阿里云 | `aliyun` | 名称、区域、AK/SK 环境变量 |
| Azure | `az` | 订阅 ID、租户 ID、区域 |
| GCP | `gcloud` | 项目 ID、区域 |

Claude 在收到查询时，会根据上下文自动判断使用哪个云平台的 CLI。例如问 "阿里云生产环境有多少台 ECS 实例"，Claude 会识别 "阿里云" 关键词，切换到 `aliyun` CLI 执行查询。

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

**仅限只读访问。** 每个 AWS 成员账号中的 `OpsAgentReadOnly` 角色绑定了 AWS 托管策略 `ReadOnlyAccess`，不能创建、修改或删除任何资源。Claude Code CLI 配置为 `--allowedTools 'Bash(git*:deny),Read,Glob,Grep'` -- 不允许任何写操作工具。

**通过 CloudFormation StackSets 在每个成员账号部署 IAM 角色。** `MemberRoleStack` 使用服务托管的 StackSet，自动部署到 Organization 中的所有账号。信任策略将 AssumeRole 限制为 Hub 账号中特定的 ECS 任务角色 ARN，并附加了外部 ID 条件作为额外保护。

```typescript
Condition: {
  StringEquals: {
    'sts:ExternalId': 'opsagent',
  },
},
```

**凭证存储在 EFS 加密卷上。** 即时通讯平台凭证（App ID、Token、Secret）通过 Admin UI 管理，持久化在 EFS 上的配置文件中。EFS 启用了静态加密，并通过 VPC 安全组限制只有 ECS 任务才能访问。不使用 Secrets Manager，减少了架构复杂度，同时通过 Admin API Key 认证保护配置修改。

**所有查询的审计日志。** 每次查询都会记录用户身份、平台、时间戳、查询文本、响应长度、耗时和成功/失败状态。日志发送到 CloudWatch Logs 和 CloudWatch Metrics，用于集中监控和告警。

**管理 API 认证。** 管理 API 的所有配置和知识库操作都需要 `X-Admin-Key` 请求头。CORS 可配置，以限制管理 UI 只能从特定来源访问。

## 快速开始

**前置条件：**
- 包含成员账号的 AWS Organization
- Hub 账号中启用 Amazon Bedrock 的 Claude Opus 模型访问
- 已安装 AWS CDK v2 和 Node.js >= 20
- 至少配置一个即时通讯平台（Teams、Slack 或飞书）

**部署步骤：**

```bash
# 第 1 步：部署主栈（VPC、ECS、ALB、EFS、IAM、ECR、CodeBuild）
cd infra && npx cdk deploy OpsAgentStack --require-approval never

# 第 2 步：在所有成员账号部署 ReadOnly 角色
npx cdk deploy MemberRoleStack -c organizationRootOuId=r-xxxx --require-approval never

# 第 3 步：构建并部署容器镜像（通过 CodeBuild，无需本地 Docker）
cd /path/to/opsagent
zip -r /tmp/source.zip . -x "*/node_modules/*" -x "*/.git/*" -x "*/cdk.out/*"
BUCKET=$(aws cloudformation describe-stacks --stack-name OpsAgentStack \
  --query 'Stacks[0].Outputs[?OutputKey==`SourceBucketName`].OutputValue' --output text)
aws s3 cp /tmp/source.zip s3://$BUCKET/source.zip
aws codebuild start-build --project-name opsagent-build

# 第 4 步：通过 Admin UI 配置平台凭证
# 访问 http://<ALB>/admin → Platform Integrations → 添加平台并填入凭证
# 然后重启服务：
aws ecs update-service --cluster opsagent-cluster --service <SERVICE> --force-new-deployment
```

部署完成后，将即时通讯平台的 Webhook URL 指向 ALB 端点（如 `http://<ALB>/api/messages/feishu`），就可以开始提问了。通过 Admin UI 管理术语表、知识库、Skills 和多云账号配置，无需重新部署。

## 未来规划

OpsAgent 已经能够满足日常运维需求，但路线图上还有更多计划：

- **通过模型路由优化成本。** 简单查询（"X 的 IP 是什么？"）使用 Claude Haiku，复杂的多账号分析保留给 Opus。这可以将典型使用场景下的 Bedrock 成本降低 60-80%。
- **流式响应提升用户体验。** 不再等待 2-3 分钟才能看到完整答案，而是在 Claude 发现结果时逐步推送中间结果。"正在扫描第 12/42 个账号... 目前已发现 3 个集群。"
- **基于角色的访问控制 (RBAC)。** 不同团队应该看到不同的账号。支付团队不需要访问市场营销的基础设施，反之亦然。
- **更多插件。** Datadog 用于指标和告警，PagerDuty 用于值班排班和事件上下文，ServiceNow 用于变更管理记录。每一个都只需要在 YAML 配置中添加一个条目。
- **带审批流程的写操作。** 精心限定范围的操作，如"重启这个 ECS 服务"或"将这个 ASG 扩展到 5 个实例"，在执行前需要人工审批。

核心理念始终如一：让 AI 代理使用已有的工具（`aws`、`aliyun`、`az`、`gcloud`、`kubectl`、MCP 插件），通过知识库和 Skills 教会它你的组织上下文。无需自定义工具开发，无需维护 CMDB。只需提出问题，即可获得答案。
