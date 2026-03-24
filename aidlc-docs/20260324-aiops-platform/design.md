# 设计文档: OpsAgent AIOps 平台演进

## 需求

用户决策:
- **方向**: 从被动查询助手进化为主动运维平台（巡检 + 检测 + RCA + 扫描）
- **存储**: PostgreSQL 存储资源、Issue、RCA 结果；YAML 配置不变
- **扫描范围**: 四朵云全覆盖（AWS、阿里云、Azure、GCP）
- **巡检触发**: Cron 定时 + 聊天命令 + 告警 Webhook（CloudWatch/Datadog）
- **RCA 展示**: 聊天回复 + Admin UI Issues 页面（历史/详情/修复建议）
- **交付方式**: 完整设计，分 3 批交付

安全基线: 已启用

## 架构

### 组件清单

| 组件 | 职责 | 新增/修改 |
|------|------|----------|
| **数据库层** | PostgreSQL 连接、ORM 模型、迁移 | 新增 |
| **资源扫描器** | 多云并行资源发现 + 资产清单 | 新增 |
| **健康巡检** | 定时健康检查、异常检测、Issue 创建 | 新增 |
| **RCA 引擎** | 根因分析 + 证据收集 + 置信度评分 | 新增 |
| **告警 Webhook** | 接入外部告警（CloudWatch/Datadog）→ 触发 RCA | 新增 |
| **增强工具集** | 20+ 专用 AWS/阿里云/Azure/GCP 工具 | 新增 |
| **Issue API + UI** | Admin UI Issues 页面、Issue 生命周期、RCA 查看器 | 新增 |
| **Claude 客户端** | Agent 循环（接口不变，注入新工具）| 修改 |
| **工具执行器** | 分发增强工具 + 扫描/巡检/RCA 工具 | 修改 |
| **调度器** | 扩展巡检任务类型 | 修改 |
| **Admin API** | 新增 Issue、资源、扫描端点 | 修改 |

### 架构图

```
                    ┌─────────────────────────────────────┐
                    │             入口层                    │
                    ├──────┬──────────┬──────────┬────────┤
                    │ 聊天 │ 定时任务 │ Webhook  │ Admin  │
                    │ (IM) │（巡检）  │（告警）  │  UI    │
                    └──┬───┴────┬─────┴────┬─────┴───┬────┘
                       │        │          │         │
                       ▼        ▼          ▼         ▼
                ┌──────────────────────────────────────────┐
                │           OpsAgent 核心                   │
                │  ┌────────────┐  ┌───────────────────┐  │
                │  │Claude 客户端│  │     调度器         │  │
                │  │(Agent 循环)│  │ (Cron + 巡检)     │  │
                │  └─────┬──────┘  └────────┬──────────┘  │
                │        │                  │              │
                │  ┌─────▼──────────────────▼──────────┐  │
                │  │          工具执行器                 │  │
                │  │  ┌─────────────────────────────┐  │  │
                │  │  │ 基础: run_command, read_file │  │  │
                │  │  │ 增强: describe_ec2,          │  │  │
                │  │  │   describe_rds, list_alarms  │  │  │
                │  │  │ 运维: scan_resources,        │  │  │
                │  │  │   run_patrol, run_rca        │  │  │
                │  │  └─────────────────────────────┘  │  │
                │  └───────────────┬────────────────────┘  │
                └──────────────────┼───────────────────────┘
                                   │
                    ┌──────────────┼──────────────┐
                    │              │              │
                    ▼              ▼              ▼
            ┌────────────┐ ┌────────────┐ ┌────────────┐
            │  资源扫描器 │ │  健康巡检   │ │  RCA 引擎  │
            │            │ │            │ │            │
            │ 多云并行    │ │ 异常检测    │ │ CloudTrail │
            │ 解析+存储   │ │ Issue 管理  │ │ 指标分析    │
            └──────┬──────┘ └─────┬──────┘ └─────┬──────┘
                   │              │              │
                   └──────────────┼──────────────┘
                                  ▼
                    ┌──────────────────────────┐
                    │      PostgreSQL 数据库     │
                    │  ┌────────┐ ┌──────────┐ │
                    │  │ 资源表 │ │ Issue 表 │ │
                    │  └────────┘ ├──────────┤ │
                    │  ┌────────┐ │ RCA      │ │
                    │  │ 扫描   │ │ 结果表   │ │
                    │  │ 日志表 │ └──────────┘ │
                    │  └────────┘              │
                    └──────────────────────────┘
```

### 数据模型

```sql
-- 扫描器发现的云资源
CREATE TABLE resources (
  id SERIAL PRIMARY KEY,
  account_name VARCHAR(100) NOT NULL,     -- 账号名称
  provider VARCHAR(20) NOT NULL,          -- aws|alicloud|azure|gcp
  region VARCHAR(50),                     -- 区域
  resource_type VARCHAR(50) NOT NULL,     -- EC2, RDS, EKS, ...
  resource_id VARCHAR(500) NOT NULL,      -- 资源唯一标识
  name VARCHAR(200),                      -- 资源名称
  status VARCHAR(50),                     -- 运行状态
  tags JSONB DEFAULT '{}',               -- 标签
  raw_data JSONB DEFAULT '{}',           -- 原始 API 响应
  tenant_id VARCHAR(50),                  -- 租户隔离
  first_seen_at TIMESTAMPTZ DEFAULT NOW(),
  last_seen_at TIMESTAMPTZ DEFAULT NOW(),
  UNIQUE(account_name, provider, resource_id)
);

-- 巡检或告警发现的健康问题
CREATE TABLE issues (
  id SERIAL PRIMARY KEY,
  resource_id VARCHAR(500),               -- 关联资源
  resource_type VARCHAR(50),
  severity VARCHAR(20) NOT NULL,          -- critical|high|medium|low
  status VARCHAR(30) DEFAULT 'open',      -- open|investigating|resolved|acknowledged
  source VARCHAR(50) NOT NULL,            -- patrol|alert|manual|chat
  title VARCHAR(500) NOT NULL,
  description TEXT,
  fingerprint VARCHAR(64),                -- 去重哈希
  occurrence_count INT DEFAULT 1,         -- 重复出现次数
  metric_data JSONB DEFAULT '{}',        -- 指标数据快照
  related_changes JSONB DEFAULT '[]',    -- 关联的 CloudTrail 变更
  tenant_id VARCHAR(50),
  account_name VARCHAR(100),
  created_at TIMESTAMPTZ DEFAULT NOW(),
  updated_at TIMESTAMPTZ DEFAULT NOW(),
  resolved_at TIMESTAMPTZ
);

-- 关联到 Issue 的 RCA 结果
CREATE TABLE rca_results (
  id SERIAL PRIMARY KEY,
  issue_id INT REFERENCES issues(id),
  root_cause TEXT NOT NULL,               -- 根因描述
  confidence FLOAT NOT NULL,              -- 置信度 0.0 - 1.0
  contributing_factors JSONB DEFAULT '[]', -- 贡献因素
  recommendations JSONB DEFAULT '[]',     -- 修复建议
  fix_plan JSONB DEFAULT '{}',           -- 修复计划
  fix_risk_level VARCHAR(20),             -- low|medium|high|critical
  evidence JSONB DEFAULT '{}',           -- 收集的日志、指标、链路
  model_id VARCHAR(100),                  -- 使用的 LLM 模型
  duration_ms INT,                        -- 分析耗时
  created_at TIMESTAMPTZ DEFAULT NOW()
);

-- 扫描执行历史
CREATE TABLE scan_logs (
  id SERIAL PRIMARY KEY,
  scan_type VARCHAR(20) NOT NULL,         -- full|incremental|patrol
  accounts_scanned JSONB DEFAULT '[]',   -- 扫描的账号列表
  resources_found INT DEFAULT 0,
  resources_updated INT DEFAULT 0,
  issues_found INT DEFAULT 0,
  duration_ms INT,
  errors JSONB DEFAULT '[]',
  created_at TIMESTAMPTZ DEFAULT NOW()
);
```

## 非功能需求计划

**性能**
- 资源扫描: 3 账号 × 8 区域 < 60 秒（并行执行）
- 巡检检查: 每账号 < 120 秒（LLM + 工具调用）
- RCA 分析: 每 Issue < 300 秒（多步调查）
- 聊天响应: 首 token < 5 秒（不变）

**可靠性**
- 扫描错误按账号/区域隔离（一个失败不影响其他）
- 巡检重试: LLM 失败重试 1 次，持续失败则跳过
- 数据库连接池: 5 连接，带退避重试

**可观测性**
- 扫描/巡检/RCA 指标写入审计日志（耗时、数量、错误）
- Issue 生命周期事件记录

**安全**
- 扫描器只读（不执行写操作）
- 所有云 API 调用经过现有沙箱 + 凭证隔离
- Webhook 端点验证来源（共享密钥或 IP 白名单）
- SQL 参数化查询（禁止字符串拼接）

## 错误/恢复映射表

| 可能失败的场景 | 错误名称 | 负责单元 | 系统行为 | 用户看到的信息 |
|---|---|---|---|---|
| 数据库连接丢失 | DatabaseUnavailable | U0: 数据库 | 重试 3 次，降级为内存模式 | "数据库暂时不可用" |
| AWS API 被限流 | ApiThrottled | U2: 扫描器 | 指数退避，重试 3 次 | 扫描继续，部分结果 |
| 扫描账号认证失败 | CredentialError | U2: 扫描器 | 跳过该账号，记录错误 | "账号 X: 认证失败" |
| 巡检 LLM 超时 | PatrolTimeout | U3: 巡检 | 跳过该账号，标记失败 | "账号 X 巡检未完成" |
| RCA 工具调用失败 | ToolCallFailed | U4: RCA | 继续用部分证据分析 | 降低置信度评分 |
| Webhook 载荷无效 | InvalidWebhook | U5: Webhook | 返回 400，记录尝试 | （外部调用方看到 400）|
| Issue 指纹冲突 | DuplicateIssue | U3: 巡检 | 增加 occurrence_count | （对用户透明）|
| 资源解析失败 | ParseError | U2: 扫描器 | 跳过该资源，记录警告 | 部分扫描结果 |
| PostgreSQL 磁盘满 | StorageFull | U0: 数据库 | 告警管理员，停止写入 | "存储容量告警" |
| 聊天巡检命令失败 | PatrolError | U3: 巡检 | 在聊天中返回错误消息 | "巡检失败: {原因}" |

## 接口契约

| 提供方 | 消费方 | 接口 | 契约（签名 + 数据结构）|
|---|---|---|---|
| U0: 数据库 | U2,U3,U4 | `db.query(sql, params) → Row[]` | 参数化 SQL，返回类型化行 |
| U2: 扫描器 | U3: 巡检 | `getResources(accountName, type?) → Resource[]` | 按账号 + 可选类型过滤 |
| U3: 巡检 | U4: RCA | `createIssue(data) → Issue` | 返回带 ID 的 Issue |
| U4: RCA | U3: 巡检 | `saveRcaResult(issueId, result) → RcaResult` | 关联 Issue，返回带 ID 的结果 |
| U5: Webhook | U4: RCA | `triggerRca(issueId) → void` | 异步触发，不等待结果 |
| U6: 工具集 | U2,U3,U4 | `executeTool(name, input, config) → ToolResult` | 与现有 tool-executor 接口一致 |

## 工作单元

| 单元 | 描述 | 依赖 | 可并行? | 批次 |
|------|------|------|---------|------|
| U0: 共享数据库 | PostgreSQL 客户端 + ORM + 迁移脚本 | 无 | 顺序（最先）| 1 |
| U1: 增强工具集 | 20+ 专用云工具（describe_ec2, list_alarms 等）| 无 | 是 | 1 |
| U2: 资源扫描器 | 多云并行扫描 + 解析 + 存储 | U0 | 是 | 1 |
| U3: 健康巡检 | 定时检查 + 异常检测 + Issue 生命周期 | U0, U2 | 否（U2 之后）| 2 |
| U4: RCA 引擎 | 根因分析 + 证据收集 + 置信度评分 | U0, U3 | 否（U3 之后）| 2 |
| U5: 告警 Webhook | 接入 CloudWatch/Datadog 告警 → 创建 Issue → 触发 RCA | U0, U3, U4 | 是 | 2 |
| U6: Issue UI | Admin UI Issues 页面 + 扫描结果 + RCA 查看器 | U0, U3, U4 | 否（U3,U4 之后）| 3 |

### 共享工具

| 工具 | 使用方 | 位置 |
|------|--------|------|
| `db.ts`（PostgreSQL 客户端）| U0, U2, U3, U4, U5, U6 | `bot/src/db.ts` |
| 云凭证解析器 | U1, U2, U3, U4 | 现有 `command-sandbox.ts` + 新增 `cloud-credentials.ts` |

## 分批交付计划

**第 1 批: 基础 + 扫描**（U0, U1, U2）
- PostgreSQL 集成
- 20+ 增强工具
- 多云资源扫描器
- 扫描结果在 Admin UI 展示
- 交付标准: 聊天中说"扫描一下所有 AWS 资源"可用

**第 2 批: 巡检 + RCA**（U3, U4, U5）
- 健康巡检 Agent（定时 + 聊天触发）
- RCA 引擎（证据收集 + 置信度评分）
- 告警 Webhook 接入
- Issue 生命周期管理
- 交付标准: 自动巡检运行，"分析这个告警"可用，Issues 页面

**第 3 批: UI + 打磨**（U6）
- Issues 列表/详情页
- RCA 结果查看器（含证据展示）
- 扫描历史仪表盘
- 资源清单浏览器
- 交付标准: 完整的 Admin UI 体验

## 决策日志

| 问题 | 决策 | 理由 |
|------|------|------|
| 产品方向 | 主动运维平台 | 用户选择: 巡检 + 检测 + RCA + 扫描 |
| 存储方案 | PostgreSQL | 用户选择: 复杂查询、并发访问、生产级别 |
| 扫描范围 | 四朵云全覆盖 | 用户选择: AWS + 阿里云 + Azure + GCP |
| 巡检触发 | Cron + 聊天 + Webhook | 用户选择: 三种入口最大覆盖 |
| RCA 展示 | 聊天 + Admin UI | 用户选择: 双重展示 |
| 交付方式 | 完整设计，3 批交付 | 用户选择: 设计一次，避免重复改架构 |

## 备选方案

| 选项 | 结论 | 原因 |
|------|------|------|
| SQLite 作为数据库 | 否决 | 用户选择 PostgreSQL，查询能力和并发更强 |
| 仅扫描 AWS | 否决 | 用户要求从一开始就覆盖四朵云 |
| 仅聊天展示（不做 Admin UI Issues 页）| 否决 | 用户要求 Issues 页面展示历史和 RCA 详情 |
| 直接移植 AgenticOps-Chat 代码 | 否决 | 技术栈不同（Python vs TypeScript），架构不同 |
| 不做设计直接开始写代码 | 否决 | Super-AIDLC 要求 Heavy 任务必须先设计 |
