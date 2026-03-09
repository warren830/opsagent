# OpsAgent - 云基础设施查询平台

## 背景

构建一个名为 "OpsAgent" 的平台，让团队可以通过 Microsoft Teams 用自然语言查询 AWS 基础设施。平台部署在 AWS Organizations 管理账号（或专用工具账号）上，接收 Teams 中的查询请求，路由到 Kiro CLI。Kiro CLI **自身已具备 AWS CLI、kubectl 等工具执行能力**，通过 Organizations 自动发现所有账号并 AssumeRole 查询，无需手动维护账号列表，也无需额外构建 MCP 工具层。

## 核心思路

> Kiro CLI 本身就能执行 `aws` CLI、`kubectl`、`eksctl` 等命令。
> 我们**不需要**为每个查询场景开发 MCP 工具，而是通过：
> 1. **知识库文件** — 告诉 Kiro CLI 如何通过 Organizations 发现账号、查询方法
> 2. **Shell 脚本** — 只对复杂的跨账号批量操作做轻量封装
> 3. **一个 Hub 账号** — 通过 Organizations + STS AssumeRole 自动访问所有成员账号
>
> 让 Kiro CLI 像一个"懂你基础设施的 SRE"一样直接工作。

## 插件化架构

OpsAgent 采用**插件化设计**，数据源通过 MCP Server 插件接入，按需启用：

```
                        ┌─────────────────────────┐
                        │       Kiro CLI           │
                        │      (Agent Mode)        │
                        └────┬────────┬────────┬───┘
                             │        │        │
              ┌──────────────┘        │        └──────────────┐
              ▼                       ▼                       ▼
    ┌─────────────────┐   ┌─────────────────┐   ┌─────────────────┐
    │  内置: Bash      │   │  插件: MCP       │   │  插件: MCP       │
    │  · aws cli      │   │  · Confluence   │   │  · Jira         │
    │  · kubectl      │   │  · 文档/Runbook  │   │  · 工单/变更记录  │
    │  · jq / grep    │   │                 │   │                 │
    └─────────────────┘   └─────────────────┘   └─────────────────┘
    ▲ 默认启用            ▲ 按需启用             ▲ 按需启用

    其他可扩展插件:
    ┌──────────┐ ┌──────────┐ ┌───────────┐ ┌───────────┐
    │ GitHub   │ │ PagerDuty│ │ Datadog   │ │ Slack     │
    │ PR/Issue │ │ 告警/oncall│ │ 监控/指标  │ │ 通知/历史  │
    └──────────┘ └──────────┘ └───────────┘ └───────────┘
```

**插件配置** (`config/plugins.yaml`)：
```yaml
plugins:
  confluence:
    enabled: true
    type: mcp
    command: "npx"
    args: ["-y", "@anthropic-ai/mcp-remote", "https://mcp.atlassian.com/v1/sse"]
    env:
      MCP_HEADERS: "Authorization: Bearer ${CONFLUENCE_TOKEN}"

  jira:
    enabled: false
    type: mcp
    command: "npx"
    args: ["-y", "@anthropic-ai/mcp-remote", "https://mcp.atlassian.com/v1/sse"]
    env:
      MCP_HEADERS: "Authorization: Bearer ${JIRA_TOKEN}"

  github:
    enabled: false
    type: mcp
    command: "npx"
    args: ["-y", "@modelcontextprotocol/server-github"]
    env:
      GITHUB_TOKEN: "${GITHUB_TOKEN}"

  # 添加新插件只需在此配置，无需改代码
```

**Kiro CLI 动态加载插件**：
```typescript
import { execFile } from 'child_process';
import { loadPlugins, generateMcpConfig } from './plugin-loader';

export async function queryInfra(userMessage: string): Promise<string> {
  // 将启用的插件写入 Kiro MCP 配置
  const plugins = loadPlugins('./config/plugins.yaml');
  generateMcpConfig(plugins, '/app/.kiro/settings/mcp.json');

  // 通过 Kiro CLI 非交互模式执行查询
  const result = await new Promise<string>((resolve, reject) => {
    execFile('kiro', [
      '--cli',
      '--prompt', userMessage,
      '--allowedTools', 'Bash,Read,Glob,Grep',
      '--system-prompt', `你是 OpsAgent，一个基础设施查询助手。
        参考知识库文件中的指引，使用 aws cli、kubectl 等工具查询 AWS 资源。
        使用 MCP 工具查询 Confluence 文档、Jira 工单等外部数据源。
        跨账号查询时使用 ./scripts/foreach-account.sh 自动遍历所有 Organizations 账号。
        始终返回结构化、易读的 Markdown 表格结果。`,
    ], { cwd: '/app' }, (err, stdout) => {
      if (err) reject(err);
      else resolve(stdout);
    });
  });
  return result;
}
```

> **添加新数据源 = 在 plugins.yaml 里加几行配置**，零代码改动。

## 架构总览

```
┌──────────┐    ┌───────────────────────────────────────────────────────────────┐
│  Teams   │    │              AWS Hub 账号 (Organizations)                      │
│  用户    │───▶│  ┌───────────┐   ┌──────────────┐   ┌────────────┐           │
│          │    │  │ Teams Bot  │──▶│  API Gateway │──▶│  ECS Task  │           │
│          │◀───│  │ (Webhook)  │◀──│  + SQS Queue │◀──│            │           │
│          │    │  └───────────┘   └──────────────┘   └─────┬──────┘           │
└──────────┘    │                                           │                  │
                │                                  ┌────────▼───────┐          │
                │                                  │   Kiro CLI     │          │
                │                                  │  (Agent Mode)  │          │
                │                                  │                │          │
                │                                  │  内置: aws cli  │          │
                │                                  │  kubectl / jq  │          │
                │                                  └──┬──────────┬──┘          │
                │                                     │          │             │
                │                          ┌──────────┘          └──────┐      │
                │                          ▼                            ▼      │
                │                 ┌─────────────────┐       ┌──────────────┐  │
                │                 │  MCP 插件        │       │  MCP 插件     │  │
                │                 │  Confluence      │       │  Jira / ...  │  │
                │                 └─────────────────┘       └──────────────┘  │
                └──────────────────────────┬──────────────────────────────────┘
                                           │
                   aws organizations list-accounts
                   +  sts:AssumeRole (自动发现, 无需配置)
                                           │
          ┌────────────────────────────────┼──────────┐
          │                                ▼          │
          │  ┌──────────┐  ┌──────────┐  ┌──────────┐│
          │  │ 账号 A   │  │ 账号 B   │  │ 账号 C   ││
          │  │EKS ECR   │  │EKS ECR   │  │EC2 RDS   ││
          │  │ELB EC2   │  │ELB EC2   │  │Route53   ││
          │  └──────────┘  └──────────┘  └──────────┘│
          │         AWS Organizations 成员账号         │
          └───────────────────────────────────────────┘
```

## 跨账号访问方案

利用 **AWS Organizations** 实现零配置的多账号访问：

```
1. Hub 账号的 ECS Task Role
   │
   │  aws organizations list-accounts  ← 自动发现所有账号
   │
   │  sts:AssumeRole  ← 统一 Role 名称，无需逐个配置
   │
   ├──▶ 账号 A: arn:aws:iam::111...:role/OpsAgentReadOnly
   ├──▶ 账号 B: arn:aws:iam::222...:role/OpsAgentReadOnly
   ├──▶ 账号 C: arn:aws:iam::333...:role/OpsAgentReadOnly
   └──▶ ...（自动发现，新增账号无需改配置）
```

**Hub 账号 IAM 策略**（仅需一条）：
```json
{
  "Effect": "Allow",
  "Action": ["sts:AssumeRole", "organizations:ListAccounts", "organizations:DescribeAccount"],
  "Resource": "*"
}
```

**每个成员账号的 IAM Role**（通过 CloudFormation StackSets 统一部署）：
```json
{
  "RoleName": "OpsAgentReadOnly",
  "AssumeRolePolicyDocument": {
    "Statement": [{
      "Effect": "Allow",
      "Principal": { "AWS": "arn:aws:iam::HUB_ACCOUNT_ID:role/OpsAgentTaskRole" },
      "Action": "sts:AssumeRole"
    }]
  },
  "ManagedPolicyArns": [
    "arn:aws:iam::aws:policy/ReadOnlyAccess"
  ]
}
```

> **新增 AWS 账号时不需要做任何配置变更** — Organizations 自动发现 + 统一 Role 名称。

## 项目结构

```
ops-agent/
├── .kiro/
│   ├── settings/
│   │   └── mcp.json                # Kiro MCP 插件配置（自动生成）
│   └── rules/
│       └── ops-agent.md            # 核心：Kiro CLI 的知识库和行为指引
├── README.md
├── infra/                          # AWS CDK 部署配置
│   ├── lib/
│   │   ├── ops-agent-stack.ts      # Hub 账号基础设施
│   │   └── member-role-stack.ts    # StackSets: 成员账号 ReadOnly Role
│   └── ...
├── bot/                            # Teams Bot 服务
│   ├── src/
│   │   ├── index.ts                # Bot 入口
│   │   ├── teams-adapter.ts        # Teams Bot Framework 适配器
│   │   ├── message-handler.ts      # 消息解析与路由
│   │   ├── kiro-client.ts           # 调用 Kiro CLI (Agent Mode)
│   │   └── plugin-loader.ts        # 读取 plugins.yaml，动态生成 mcpServers 配置
│   ├── package.json
│   └── Dockerfile
├── config/
│   └── plugins.yaml                # 插件配置（启用/禁用 MCP 数据源）
├── scripts/                        # 辅助脚本（仅用于复杂跨账号批量操作）
│   ├── foreach-account.sh          # 核心：自动发现账号 + 遍历执行命令
│   ├── list-all-clusters.sh        # 遍历所有账号列出 EKS 集群
│   ├── scan-expiring-certs.sh      # 跨账号扫描即将过期的 ACM 证书
│   └── find-ip-across-accounts.sh  # 跨账号查找 IP 归属
└── tests/
    └── integration/                # 集成测试
```

## 实施计划

### 第一阶段：Kiro 知识库 + Organizations 跨账号配置（核心）

#### 1.1 成员账号 ReadOnly Role（通过 StackSets 统一部署）
- 使用 AWS CloudFormation StackSets 在所有成员账号创建 `OpsAgentReadOnly` Role
- 信任 Hub 账号的 ECS Task Role
- 附加 `ReadOnlyAccess` 托管策略

#### 1.2 跨账号遍历脚本

`scripts/foreach-account.sh` — 核心脚本，自动发现并遍历：
```bash
#!/bin/bash
# 自动从 Organizations 获取所有账号，对每个账号 AssumeRole 后执行命令
ROLE_NAME="OpsAgentReadOnly"

accounts=$(aws organizations list-accounts --query 'Accounts[?Status==`ACTIVE`].[Id,Name]' --output text)

while read -r account_id account_name; do
  echo "=== $account_name ($account_id) ==="

  creds=$(aws sts assume-role \
    --role-arn "arn:aws:iam::${account_id}:role/${ROLE_NAME}" \
    --role-session-name "ops-agent" \
    --output json)

  export AWS_ACCESS_KEY_ID=$(echo $creds | jq -r '.Credentials.AccessKeyId')
  export AWS_SECRET_ACCESS_KEY=$(echo $creds | jq -r '.Credentials.SecretAccessKey')
  export AWS_SESSION_TOKEN=$(echo $creds | jq -r '.Credentials.SessionToken')

  eval "$1"  # 执行传入的命令

  unset AWS_ACCESS_KEY_ID AWS_SECRET_ACCESS_KEY AWS_SESSION_TOKEN
done <<< "$accounts"
```

使用示例：
```bash
# 列出所有账号的 EKS 集群
./scripts/foreach-account.sh aws eks list-clusters --output json

# 查找某个 IP 属于哪个账号
./scripts/foreach-account.sh aws ec2 describe-addresses --filters Name=public-ip,Values=1.2.3.4
```

#### 1.3 Kiro Rules (`.kiro/rules/ops-agent.md`) — 告诉 Kiro CLI 怎么查

```markdown
# OpsAgent - AWS 基础设施查询助手

## 账号结构
- 我们使用 AWS Organizations，Hub 账号可以自动发现所有成员账号
- 发现账号：`aws organizations list-accounts --query 'Accounts[?Status==ACTIVE]'`
- 访问账号：`aws sts assume-role --role-arn arn:aws:iam::<account_id>:role/OpsAgentReadOnly`
- 跨账号批量查询：使用 `./scripts/foreach-account.sh aws <命令> [参数...]`

## 常用查询方法

### K8s / EKS
- 查集群出口 IP：先 describe-cluster 拿 VPC → 查 NAT Gateway 的 EIP
- 按版本列集群：`./scripts/foreach-account.sh aws eks list-clusters` → 对每个集群 describe 取版本
- 查 namespace/deployment：先 `aws eks update-kubeconfig`，再 kubectl
- 查 deployment 版本：`kubectl get deploy <name> -n <ns> -o jsonpath='{..image}'`

### ECR
- 查镜像是否存在：`aws ecr describe-images --repository-name <repo> --image-ids imageTag=<tag>`

### 域名 / Route 53
- 查域名绑定资源：`aws route53 list-resource-record-sets` 过滤后看 CNAME/ALIAS 指向
- 查实例绑定域名数：反向查 Route 53 记录中指向该资源的条目

### 证书 / ACM
- 查本月过期证书：`./scripts/foreach-account.sh aws acm list-certificates` → 过滤 NotAfter
- 查 CloudFront 关联证书过期：get-distribution → ViewerCertificate → ACM ARN
- 查 Ingress 关联证书过期：`kubectl get ingress -A -o json` → 提取 tls → 查 ACM

### 网络
- 查 EIP 归属：`./scripts/foreach-account.sh aws ec2 describe-addresses --filters Name=public-ip,Values=<ip>`
- 查 IP 在哪个 Security Group：describe-security-groups → 遍历 IpPermissions
- 查 CIDR 归属：`./scripts/foreach-account.sh aws ec2 describe-vpcs` + describe-subnets

### EC2 / 操作系统
- 统计 CentOS 7：describe-instances → 按 PlatformDetails 或 AMI Name 过滤
- 统计关闭实例：`--filters Name=instance-state-name,Values=stopped`

### Security Group / ACL
- 查 ELB 的 SG IP 规则：describe-load-balancers → 拿 SG → describe-security-groups
- 查 CloudFront WAF IP：`aws wafv2 list-ip-sets --scope CLOUDFRONT` → get-ip-set
- 查 RDS 的 SG IP 规则：describe-db-instances → 拿 SG → describe-security-groups

### Confluence（通过 MCP 插件）
- 查架构文档：使用 Confluence MCP 工具搜索 space 和页面
- 查 Runbook：搜索操作手册、故障处理流程
- 查变更记录：搜索变更相关页面
- 当用户问"这个服务的架构是什么"或"有没有相关文档"时，优先查 Confluence

## 输出规范
- 查询结果以 Markdown 表格返回
- 跨账号查询时标注账号名和账号 ID
- 引用 Confluence 文档时附上页面链接
- 如果结果为空，明确说明"未找到"
```

---

### 第二阶段：Teams Bot 服务

#### 2.1 Teams Bot 搭建
- 注册 Azure Bot Service（用于 Teams 频道）
- 使用 TypeScript 实现 Bot Framework 适配器（`botbuilder` SDK）
- 处理来自 Teams 对话的传入消息

#### 2.2 Kiro CLI 集成 (`kiro-client.ts`)

```typescript
import { execFile } from 'child_process';
import { loadPlugins, generateMcpConfig } from './plugin-loader';

export async function queryInfra(userMessage: string): Promise<string> {
  // 将启用的插件写入 Kiro MCP 配置
  const plugins = loadPlugins('./config/plugins.yaml');
  generateMcpConfig(plugins, '/app/.kiro/settings/mcp.json');

  // 通过 Kiro CLI 非交互模式执行查询
  const result = await new Promise<string>((resolve, reject) => {
    execFile('kiro', [
      '--cli',
      '--prompt', userMessage,
      '--allowedTools', 'Bash,Read,Glob,Grep',
    ], { cwd: '/app' }, (err, stdout) => {
      if (err) reject(err);
      else resolve(stdout);
    });
  });
  return result;
}
```

#### 2.3 响应格式化
- 将 Markdown 结果格式化为 Teams Adaptive Card（表格、列表）
- 支持长时间查询的流式响应
- 错误处理和超时管理

---

### 第三阶段：AWS 部署基础设施

#### 3.1 计算资源
**推荐 ECS Fargate**，原因：
- Kiro CLI 长时间会话可能超过 Lambda 的 15 分钟超时限制
- 需要安装 aws cli、kubectl、jq 等工具的运行环境

#### 3.2 Dockerfile

```dockerfile
FROM node:20-slim
RUN apt-get update && apt-get install -y curl unzip jq
# AWS CLI
RUN curl "https://awscli.amazonaws.com/awscli-exe-linux-x86_64.zip" -o "awscliv2.zip" \
    && unzip awscliv2.zip && ./aws/install
# kubectl
RUN curl -LO "https://dl.k8s.io/release/stable.txt" \
    && curl -LO "https://dl.k8s.io/release/$(cat stable.txt)/bin/linux/amd64/kubectl" \
    && install kubectl /usr/local/bin/kubectl
# Kiro CLI
RUN npm install -g kiro-cli
COPY . /app
WORKDIR /app
RUN npm install
CMD ["node", "bot/dist/index.js"]
```

#### 3.3 基础设施组件
- **ALB** → ECS Service（Teams Bot webhook 端点）
- **Secrets Manager** → Teams Bot Token、Confluence Token 等插件凭证
- **IAM Task Role** → 允许 organizations:List* + sts:AssumeRole
- **CloudWatch** → 日志和监控

#### 3.4 IaC
- 使用 AWS CDK (TypeScript)
- **Hub Stack**：VPC + ECS Cluster + Service + ALB + Secrets + IAM
- **StackSets**：在所有成员账号部署 OpsAgentReadOnly Role

---

### 第四阶段：安全与运维

#### 4.1 凭证管理
- ECS Task Role → Organizations ListAccounts → STS AssumeRole（零 AccessKey）
- 成员账号 IAM Role 附加 `ReadOnlyAccess` 托管策略
- Teams Bot Token / 插件凭证 存储在 Secrets Manager

#### 4.2 访问控制
- Teams Bot：限制为特定 Teams 频道/用户
- Kiro CLI allowedTools 仅开放 Bash(只读命令)、Read、Glob、Grep
- 审计日志：记录每次查询的用户身份、时间戳、查询内容
- CloudTrail：自动记录所有 AWS API 调用

#### 4.3 监控
- CloudWatch 告警：错误率、延迟
- 查询审计追踪：CloudWatch Logs / DynamoDB

---

## 建议实施顺序

```
第 1 周：阶段 1    → StackSets 部署 Role + Kiro Rules + 辅助脚本 + 本地验证
第 2 周：阶段 2    → Teams Bot + Kiro CLI 集成
第 3 周：阶段 3    → ECS Fargate 部署 + Dockerfile
第 4 周：阶段 4    → IAM 权限收紧 + 监控 + 上线
```

## 验证方案

1. **本地验证**：在本地 Kiro CLI 中加载知识库规则，直接用自然语言测试所有查询场景
2. **Teams 端到端测试**：在 Teams 中发送消息 → 验证返回的数据正确性
3. **示例测试查询**：
   - "ics-stg-aps1 的出口 IP 是什么？"
   - "有多少个 EKS 集群是 v1.31 版本？"
   - "vip.invisalign.net.cn 绑定在哪些资源上？"
   - "这个月有哪些 ACM 证书要过期？"
   - "ELB 的 Security Group 里有哪些 IP？"

## 方案演进对比

| 维度 | V1 (MCP Server) | V2 (CLI Profiles) | V3 (Organizations) | V4 (插件化) |
|------|------------------|--------------------|---------------------|-------------|
| 账号发现 | 手动 yaml | 手动 profiles | 自动发现 | **自动发现** |
| 新增账号 | 改代码+配置 | 改 yaml+profiles | 零配置 | **零配置** |
| 工具层 | 18 个 MCP 工具 | aws cli 直接调 | aws cli 直接调 | **aws cli + MCP 插件** |
| 外部数据源 | 无 | 无 | 无 | **Confluence/Jira/GitHub/...** |
| 新增数据源 | 开发新工具 | 不支持 | 不支持 | **改 plugins.yaml** |
| 开发量 | 10+ 文件 | 知识库+脚本 | 知识库+脚本 | **知识库+脚本+loader** |
| 交付周期 | 5-6 周 | 3-4 周 | 3-4 周 | **3-4 周** |
