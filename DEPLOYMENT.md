# OpsAgent 部署方案

## 1. 系统架构

```
                         ┌─────────────────────────────────────┐
                         │         AWS Hub Account             │
                         │         (034362076319)              │
  ┌──────────┐           │                                     │
  │  飞书     │──webhook──│──▶ ALB (:80)                       │
  │  Teams   │──webhook──│──▶   └─▶ ECS Fargate (4C/8G)      │
  │  Slack   │──webhook──│──▶         ├─ Node.js Bot Server   │
  └──────────┘           │            ├─ Claude Code CLI       │
                         │            │    └─▶ Bedrock (Opus)  │
  ┌──────────┐           │            └─ EFS /app/knowledge    │
  │ Admin UI │──http────-│──▶ ALB /admin                      │
  └──────────┘           │                                     │
                         │  CloudWatch Logs (/ecs/opsagent)   │
                         │  ECR (opsagent:latest)             │
                         └──────────┬──────────────────────────┘
                                    │ sts:AssumeRole
                    ┌───────────────┼───────────────┐
                    ▼               ▼               ▼
              ┌──────────┐   ┌──────────┐   ┌──────────┐
              │ Member A │   │ Member B │   │ External │
              │ ReadOnly │   │ ReadOnly │   │  Custom  │
              │  Role    │   │  Role    │   │  Role    │
              └──────────┘   └──────────┘   └──────────┘
```

## 2. 项目结构

```
opsagent/
├── bot/                          # Node.js Bot 应用
│   ├── src/
│   │   ├── index.ts              # HTTP Server, 路由, 平台注册
│   │   ├── claude-client.ts      # Claude Code CLI 调用, 会话管理
│   │   ├── message-handler.ts    # 平台无关的消息处理
│   │   ├── admin-api.ts          # Admin REST API (配置 CRUD + 知识库管理)
│   │   ├── platform-loader.ts    # 加载 platforms.yaml
│   │   ├── plugin-loader.ts      # 加载 plugins.yaml, 生成 mcp.json
│   │   ├── glossary-loader.ts    # 加载 glossary.yaml, 生成 knowledge/glossary.md
│   │   ├── accounts-loader.ts    # 加载 accounts.yaml, 生成 knowledge/accounts.md
│   │   └── adapters/
│   │       ├── types.ts          # PlatformAdapter / PlatformMessage 接口
│   │       ├── teams.ts          # Microsoft Teams (Bot Framework)
│   │       ├── slack.ts          # Slack (Events API)
│   │       └── feishu.ts         # 飞书 (Event Subscription v2)
│   ├── static/
│   │   └── admin.html            # Admin UI (单文件, 4 Tab + Chat)
│   ├── package.json
│   └── tsconfig.json
├── config/                       # YAML 配置 (构建时烧入镜像)
│   ├── platforms.yaml            # IM 平台开关
│   ├── glossary.yaml             # 公司术语黑话库
│   ├── accounts.yaml             # 额外 AWS 账号 + 覆盖
│   └── plugins.yaml              # MCP 插件 (Confluence/Jira/GitHub)
├── knowledge/                    # 知识库 (EFS 持久化)
├── scripts/
│   ├── foreach-account.sh        # 遍历 Organization 所有账号执行命令
│   ├── list-all-clusters.sh
│   ├── find-ip-across-accounts.sh
│   └── scan-expiring-certs.sh
├── infra/                        # CDK 基础设施
│   ├── bin/app.ts                # CDK App 入口
│   └── lib/
│       ├── ops-agent-stack.ts    # 主 Stack (VPC/ECS/ALB/EFS/IAM)
│       └── member-role-stack.ts  # StackSet: 成员账号 ReadOnly Role
├── Dockerfile
└── DEPLOYMENT.md                 # 本文件
```

## 3. 前置条件

| 依赖项 | 版本 | 用途 |
|--------|------|------|
| Node.js | >= 20 | Bot 运行时 |
| AWS CLI v2 | latest | CDK 部署 + 容器内查询 |
| Docker | >= 24 | 构建镜像 |
| AWS CDK | >= 2.x | 基础设施部署 |

AWS 账号要求:
- Hub 账号已启用 AWS Organizations
- Hub 账号已启用 Amazon Bedrock 并开通 Claude Opus 4.6 模型访问
- ECR 仓库 `opsagent` 已创建

## 4. 部署步骤

### 4.1 首次部署基础设施

```bash
# 设置环境变量
export HUB_ACCOUNT_ID=034362076319
export CDK_DEFAULT_REGION=us-east-1

# 创建 ECR 仓库 (仅首次)
aws ecr create-repository --repository-name opsagent --region us-east-1

# CDK Bootstrap (仅首次)
cd infra
npx cdk bootstrap aws://$HUB_ACCOUNT_ID/$CDK_DEFAULT_REGION

# 部署主 Stack
npx cdk deploy OpsAgentStack --require-approval never

# (可选) 部署成员账号角色 StackSet
npx cdk deploy MemberRoleStack \
  -c organizationRootOuId=r-xxxx \
  --require-approval never
```

### 4.2 配置平台凭证

平台凭证通过 Admin UI 或直接编辑 `config/platforms.yaml` 管理，不再使用 Secrets Manager。
部署完成后访问 `http://<ALB>/admin` → Platforms 标签页，添加所需的 IM 平台集成并填入 credentials。

### 4.3 构建并推送镜像

```bash
# 登录 ECR
aws ecr get-login-password --region us-east-1 | \
  docker login --username AWS --password-stdin \
  $HUB_ACCOUNT_ID.dkr.ecr.us-east-1.amazonaws.com

# 构建 (从项目根目录)
docker build --platform linux/amd64 -t opsagent:latest .

# Tag & Push
docker tag opsagent:latest \
  $HUB_ACCOUNT_ID.dkr.ecr.us-east-1.amazonaws.com/opsagent:latest
docker push \
  $HUB_ACCOUNT_ID.dkr.ecr.us-east-1.amazonaws.com/opsagent:latest
```

### 4.4 启动/更新服务

```bash
# 获取 Service 名称
SERVICE=$(aws ecs list-services --cluster opsagent-cluster \
  --query 'serviceArns[0]' --output text --region us-east-1)

# 滚动更新
aws ecs update-service \
  --cluster opsagent-cluster \
  --service $SERVICE \
  --force-new-deployment \
  --region us-east-1

# 等待稳定
aws ecs wait services-stable \
  --cluster opsagent-cluster \
  --services $SERVICE \
  --region us-east-1
```

### 4.5 验证

```bash
# 获取 ALB 地址
ALB=$(aws cloudformation describe-stacks --stack-name OpsAgentStack \
  --query 'Stacks[0].Outputs[?OutputKey==`AlbDnsName`].OutputValue' \
  --output text --region us-east-1)

# 健康检查
curl http://$ALB/health

# Admin UI
open http://$ALB/admin
```

## 5. AWS 资源清单

### OpsAgentStack 创建的资源

| 资源 | 类型 | 说明 |
|------|------|------|
| OpsAgentVpc | VPC | 2 AZ, Public + Private 子网, 1 NAT Gateway |
| OpsAgentCluster | ECS Cluster | Fargate 集群 |
| OpsAgentTaskDef | Task Definition | 4 vCPU / 8 GB Memory |
| OpsAgentService | Fargate Service | desiredCount=1, ECS Exec 已启用 |
| OpsAgentAlb | ALB | 公网 HTTP:80 |
| OpsAgentEfs | EFS | 加密, 知识库持久存储, RETAIN 策略 |
| OpsAgentTaskRole | IAM Role | ReadOnlyAccess + AssumeRole + Bedrock + EFS |
| OpsAgentExecutionRole | IAM Role | ECS 执行角色 (拉镜像/写日志) |
| /ecs/opsagent | CloudWatch Log Group | 30 天保留 |

### IAM 权限矩阵 (TaskRole)

| 权限 | 范围 | 用途 |
|------|------|------|
| ReadOnlyAccess | Hub 账号所有服务 | 查询 AWS 资源 |
| sts:AssumeRole | arn:aws:iam::*:role/OpsAgentReadOnly | 跨账号查询 |
| bedrock:InvokeModel* | 所有 Bedrock 模型 | Claude Code 调用 LLM |
| elasticfilesystem:Client* | OpsAgentEfs | 知识库及配置读写 |

## 6. 配置管理

### 6.1 平台集成 (config/platforms.yaml)

通过 Admin UI → Platform Integrations 添加平台，填入 credentials 和 settings。也可直接编辑配置文件：

```yaml
platforms:
  teams:
    enabled: true
    credentials:
      app_id: "your-microsoft-app-id"
      app_password: "your-microsoft-app-password"
    settings:
      welcome_message: "OpsAgent ready."
  slack:
    enabled: true
    credentials:
      bot_token: "xoxb-..."
      signing_secret: "your-signing-secret"
    settings:
      allowed_channels: []   # 空 = 所有 channel
  feishu:
    enabled: true
    credentials:
      app_id: "cli_xxxx"
      app_secret: "xxxx"
      verification_token: "xxxx"
    settings: {}
```

修改后需 ECS force-new-deployment 生效（平台注册在启动时完成）。

### 6.2 公司术语 (config/glossary.yaml)

```yaml
glossary:
  ics:
    full_name: "Inventory Control System"
    description: "库存管理核心系统，部署在 EKS 上"
    aliases: ["inventory-system"]
    aws_accounts: ["123456789012"]
    services: ["ics-api", "ics-worker"]
```

术语摘要会注入到 Claude Code 的 system prompt 中。
可通过 Admin UI → Glossary Tab 在线编辑，立即生效 (下次查询读取)。

### 6.3 AWS 账号 (config/accounts.yaml)

```yaml
accounts:
  extra:                         # Organization 外的额外账号
    - id: "111111111111"
      name: "partner-staging"
      role_name: "OpsAgentPartnerRole"   # 该账号的自定义角色
      regions: ["us-east-1"]
  overrides:                     # 覆盖 Organization 账号的设置
    "034362076319":
      alias: "hub-account"
    "999999999999":
      skip: true                 # 跳过此账号
  defaults:
    role_name: "OpsAgentReadOnly"
    regions: ["us-east-1"]
```

可通过 Admin UI → Accounts Tab 在线编辑。

### 6.4 MCP 插件 (config/plugins.yaml)

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

每次查询时 plugin-loader 根据此配置生成 `config/mcp.json` 供 Claude Code 使用。

## 7. 知识库

### 存储

- **位置**: EFS → 容器内 `/app/knowledge/`
- **持久化**: 容器重启/重新部署不丢失
- **自动生成**: `glossary.md` 和 `accounts.md` 由 loader 自动生成
- **用户文件**: 通过 Admin UI → Knowledge Tab 上传/创建/编辑/删除

### Claude Code 如何使用

Claude Code 拥有 `Glob`、`Grep`、`Read` 工具，system prompt 中提示:

> knowledge/ 目录包含公司的知识库文件（runbook、架构文档等），
> 遇到不确定的问题时先用 Grep 在该目录搜索相关内容，再用 Read 读取匹配的文件。

Claude Code 按需自行搜索，不预加载全部内容，无 token 浪费。

### 管理方式

| 方式 | 操作 |
|------|------|
| Admin UI | `http://<ALB>/admin` → Knowledge Tab: 拖拽上传 / 新建 / 编辑 / 删除 |
| API | `GET /admin/api/knowledge` 列出文件 |
| | `GET /admin/api/knowledge/:filename` 读取内容 |
| | `PUT /admin/api/knowledge/:filename` 创建/更新 |
| | `DELETE /admin/api/knowledge/:filename` 删除 |
| ECS Exec | `aws ecs execute-command --cluster opsagent-cluster --task <taskId> --container opsagent --interactive --command /bin/bash` 直接操作文件系统 |

## 8. 会话管理

- 每个用户 (platform + userId) 维护一个 Claude Code session
- 使用 `--resume <sessionId>` 实现多轮对话上下文延续
- Session 空闲 30 分钟自动过期，下次查询创建新 session
- Session 数据存储在容器内存中，容器重启后所有 session 重置

## 9. API 端点

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/health` | 健康检查, 返回已启用的平台列表 |
| POST | `/api/messages/teams` | Teams Bot Webhook |
| POST | `/api/messages/slack` | Slack Events Webhook |
| POST | `/api/messages/feishu` | 飞书 Events Webhook |
| POST | `/api/messages` | 兼容旧路由 → Teams |
| GET | `/admin` | Admin UI |
| POST | `/admin/api/chat` | Admin Chat (直接查询 Claude) |
| GET/PUT | `/admin/api/glossary` | 术语配置 |
| GET/PUT | `/admin/api/accounts` | 账号配置 |
| GET/PUT | `/admin/api/platforms` | 平台配置 |
| GET | `/admin/api/knowledge` | 列出知识库文件 |
| GET/PUT/DELETE | `/admin/api/knowledge/:filename` | 知识库文件 CRUD |

## 10. 日常运维

### 查看日志

```bash
aws logs tail /ecs/opsagent --follow --region us-east-1
```

### 进入容器调试

```bash
TASK=$(aws ecs list-tasks --cluster opsagent-cluster \
  --query 'taskArns[0]' --output text --region us-east-1)

aws ecs execute-command \
  --cluster opsagent-cluster \
  --task $TASK \
  --container opsagent \
  --interactive \
  --command /bin/bash
```

### 更新代码后重新部署

```bash
# 1. 构建推送
docker build --platform linux/amd64 -t opsagent:latest .
docker tag opsagent:latest $HUB_ACCOUNT_ID.dkr.ecr.us-east-1.amazonaws.com/opsagent:latest
docker push $HUB_ACCOUNT_ID.dkr.ecr.us-east-1.amazonaws.com/opsagent:latest

# 2. 滚动更新
aws ecs update-service --cluster opsagent-cluster \
  --service $SERVICE --force-new-deployment --region us-east-1
```

### 更新基础设施

```bash
cd infra && npx cdk deploy OpsAgentStack --require-approval never
```

## 11. 添加外部 AWS 账号 (非 Organization)

1. 在目标账号创建 IAM Role:
   - 角色名: 自定义 (如 `OpsAgentPartnerRole`)
   - Trust Policy: 允许 `arn:aws:iam::034362076319:role/OpsAgentTaskRole` AssumeRole
   - 权限: `ReadOnlyAccess` (或按需缩小)

2. 在 Admin UI → Accounts → Extra Accounts 添加:
   - Account ID: 目标账号 ID
   - Name: 可读名称
   - Role Name: 上面创建的角色名
   - Regions: 需要查询的区域

## 12. 当前部署信息

| 项目 | 值 |
|------|-----|
| Hub Account | 034362076319 |
| Region | us-east-1 |
| ALB | opsagent-alb-397388398.us-east-1.elb.amazonaws.com |
| ECS Cluster | opsagent-cluster |
| ECR | 034362076319.dkr.ecr.us-east-1.amazonaws.com/opsagent |
| EFS | fs-032b95050399865fe |
| Log Group | /ecs/opsagent |
| LLM | Claude Opus 4.6 via Bedrock |
| Task Spec | 4 vCPU / 8 GB Memory |
| Admin UI | http://opsagent-alb-397388398.us-east-1.elb.amazonaws.com/admin |
