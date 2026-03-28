# 设计文档: Personal Knowledge/Skills/调度 + ALB Sticky Session

## 需求

用户决策:
- **Personal Knowledge**: 每个用户可上传/管理自定义 Markdown 知识文件，查询时自动注入到系统提示中
- **Personal Skills**: 用户可自定义技能 + 从全局技能池中启用/禁用特定技能
- **Personal 调度**: 用户可创建个人定时任务（结果只发给自己）+ ALB 粘性 Session 确保路由一致
- **ALB Sticky Session**: ALB Cookie-based 粘性，duration-based（1 天），确保水平扩展后同一用户始终路由到同一 ECS Task
- **三层继承**: 全局 → 租户 → 用户。个人配置继承并可覆盖租户配置，租户配置继承并可覆盖全局配置
- **安全基线**: 已启用

### 继承层级

```
全局 (config/skills.yaml, knowledge/*.md)
  └─ 租户 (knowledge/_tenants/{tenant_id}/_config/skills.yaml)
       └─ 用户 (knowledge/_users/{username}/_config/skills.yaml)
```

**合并规则**:
- Knowledge: 全局知识 ∪ 租户知识 ∪ 用户知识（并集，同名文件用户优先）
- Skills: 全局技能池 → 租户 override → 用户 override → 用户 custom（逐层覆盖）
- 调度: 全局调度 + 用户个人调度（并集，不互斥）

## 架构

### 组件清单

| 组件 | 职责 | 新增/修改 |
|------|------|----------|
| **ALB Sticky Session** | CDK Target Group 添加粘性 Cookie | 修改 |
| **CloudFront Cookie 转发** | 确保 ALB 粘性 Cookie 通过 CF 传递 | 修改 |
| **用户配置加载器** | 加载/管理 `knowledge/_users/{username}/` 目录结构 | 新增 |
| **个人知识系统** | 用户级知识文件扫描 + 系统提示注入 | 修改 |
| **个人技能系统** | 用户级技能配置（自定义 + 启用/禁用） | 修改 |
| **个人调度系统** | 用户级定时任务（结果发送到用户 IM 会话） | 修改 |
| **Admin API 扩展** | 用户 Knowledge/Skills/Jobs CRUD 端点 | 修改 |
| **Admin UI 扩展** | 个人配置管理界面（Knowledge/Skills/Jobs） | 修改 |

### 架构图

```
                    ┌──────────────────────────────────────────────┐
                    │                CloudFront                     │
                    │  (转发 AWSALB Cookie, Cookie Whitelist)      │
                    └────────────────────┬─────────────────────────┘
                                         │
                    ┌────────────────────▼─────────────────────────┐
                    │        ALB (Sticky Session Enabled)           │
                    │  Cookie: AWSALB, Duration: 1 day              │
                    │  同一用户 → 同一 ECS Task                     │
                    └────┬──────────────────────────────┬──────────┘
                         │                              │
                ┌────────▼────────┐            ┌───────▼─────────┐
                │   ECS Task A     │            │   ECS Task B     │
                │                  │            │                  │
                │  ┌─────────────┐ │            │  ┌─────────────┐│
                │  │ SessionStore│ │            │  │ SessionStore││
                │  │ (in-memory) │ │            │  │ (in-memory) ││
                │  └─────────────┘ │            │  └─────────────┘│
                │  ┌─────────────┐ │            │  ┌─────────────┐│
                │  │ClaudeClient │ │            │  │ClaudeClient ││
                │  │  Sessions   │ │            │  │  Sessions   ││
                │  └─────────────┘ │            │  └─────────────┘│
                └────────┬─────────┘            └───────┬─────────┘
                         │                              │
                         └──────────────┬───────────────┘
                                        │
                    ┌───────────────────▼──────────────────────────┐
                    │           EFS (共享存储)                      │
                    │                                              │
                    │  knowledge/                                   │
                    │  ├── _users/                                  │
                    │  │   ├── alice/                               │
                    │  │   │   ├── _config/                        │
                    │  │   │   │   ├── skills.yaml                 │
                    │  │   │   │   └── scheduled-jobs.yaml         │
                    │  │   │   ├── my-runbook.md                   │
                    │  │   │   └── my-notes.md                     │
                    │  │   └── bob/                                 │
                    │  │       ├── _config/                        │
                    │  │       │   └── skills.yaml                 │
                    │  │       └── custom-kb.md                    │
                    │  ├── _tenants/                               │
                    │  ├── _config/                                │
                    │  ├── runbook-xxx.md                          │
                    │  └── ...                                     │
                    └──────────────────────────────────────────────┘
```

### 数据模型

**用户级目录结构** (EFS, 延续现有 `_tenants/` 模式):

```
knowledge/_users/{username}/
├── _config/
│   ├── skills.yaml          # 个人技能配置
│   └── scheduled-jobs.yaml  # 个人定时任务
├── *.md                      # 个人知识文件
```

**个人技能配置** (`skills.yaml`):
```yaml
# 从全局技能池中启用/禁用
overrides:
  - name: "ECS Troubleshooting"
    enabled: false              # 覆盖全局: 禁用
  - name: "Jira Integration"
    enabled: true               # 保持启用（默认跟随全局）

# 用户自定义技能
custom_skills:
  - name: "My Deploy Checklist"
    description: "个人部署检查清单"
    instructions: |
      部署前确认:
      1. 检查 CloudWatch 告警
      2. 确认 RDS 快照
      ...
```

**个人调度配置** (`scheduled-jobs.yaml`):
```yaml
scheduled_jobs:
  - name: "我的每日巡检"
    enabled: true
    cron: "0 9 * * 1-5"
    timezone: "Asia/Shanghai"
    query: "检查 ics 账号的 EC2 实例利用率"
    # 无 targets — 结果存储到用户的 job 历史，下次登录 Admin UI 可查看
```

## NFR 计划

- [x] 响应时间: 知识/技能加载 < 200ms（EFS 读取 + YAML 解析）
- [x] 并发: 利用 ALB sticky session，会话不跨 Task
- [x] 数据持久化: EFS 持久存储，Task 重启不丢失
- [x] 日志: 复用现有 audit-logger
- [x] 缓存: 暂无需要（EFS 读取足够快）

## Error/Rescue Map

| 故障场景 | 错误名 | 责任单元 | 系统行为 | 用户感知 |
|----------|--------|----------|----------|----------|
| 用户目录不存在 | UserDirNotFound | U1: UserConfigLoader | 自动创建 `_users/{username}/` + `_config/` | 无感知，首次使用自动初始化 |
| 个人知识文件格式错误 | InvalidKnowledgeFile | U2: PersonalKnowledge | 跳过无法解析的文件，打日志 | 系统提示中缺少该文件 |
| 个人技能 YAML 解析失败 | InvalidSkillsConfig | U3: PersonalSkills | 回退到全局技能配置 | 显示全局技能，Admin UI 警告 |
| 个人调度 cron 表达式无效 | InvalidCronExpression | U4: PersonalScheduler | 拒绝注册，日志告警 | Admin UI 显示验证错误 |
| ALB Sticky Cookie 丢失 | StickySessionLost | U0: ALBSticky | 用户被路由到新 Task，内存会话丢失 | 会话中断，需重新发起对话 |
| EFS 读取超时 | EfsReadTimeout | U1: UserConfigLoader | 回退到默认配置（无个人化） | 暂时无个人知识/技能 |
| 用户上传文件名包含路径穿越 | PathTraversalAttempt | U2: PersonalKnowledge | 拒绝并返回 400 错误 | "文件名无效" |
| CloudFront 未转发 ALB Cookie | CookieNotForwarded | U0: ALBSticky | sticky 失效，轮询分发 | 频繁会话中断 |

## Interface Contracts

| Provider Unit | Consumer Unit | 接口 | 契约 |
|--------------|--------------|------|------|
| U1: UserConfigLoader | U2: PersonalKnowledge | `getUserKnowledgeDir(username: string): string` | 返回 `knowledge/_users/{username}/`，自动创建 |
| U1: UserConfigLoader | U3: PersonalSkills | `getUserConfigPath(username: string, file: string): string` | 返回 `knowledge/_users/{username}/_config/{file}` |
| U1: UserConfigLoader | U4: PersonalScheduler | `getUserConfigPath(username: string, file: string): string` | 同上 |
| U3: PersonalSkills | SystemPromptBuilder | `loadMergedSkills(globalPath: string, tenantId?: string, username?: string): SkillsConfig` | 三层合并: 全局 → 租户 override → 用户 override + custom skills |
| U2: PersonalKnowledge | SystemPromptBuilder | `scanMergedKnowledge(knowledgeDir: string, tenantId?: string, username?: string): KnowledgeFileEntry[]` | 三层并集: 全局 ∪ 租户 ∪ 用户知识文件 |

## 单元分解

| 单元 | 描述 | 依赖 | 可并行? | 批次 |
|------|------|------|--------|------|
| U0: ALB Sticky Session | CDK: Target Group 粘性 + CF Cookie 转发 | 无 | 是 | Batch 1 |
| U1: User Config Loader | 用户级目录初始化 + 配置路径工具函数 | 无 | 是 | Batch 1 |
| U2: Personal Knowledge | 个人知识扫描 + 系统提示注入 + Admin API | U1 | 是 (与 U3, U4 并行) | Batch 2 |
| U3: Personal Skills | 个人技能合并 + Admin API | U1 | 是 (与 U2, U4 并行) | Batch 2 |
| U4: Personal Scheduler | 用户级定时任务 + Admin API | U1 | 是 (与 U2, U3 并行) | Batch 2 |

### 共享工具 (U1)

| 工具函数 | 使用者 | 位置 |
|---------|--------|------|
| `ensureUserDir(username)` | U2, U3, U4 | `bot/src/user-config-loader.ts` |
| `getUserKnowledgeDir(username)` | U2 | `bot/src/user-config-loader.ts` |
| `getUserConfigPath(username, file)` | U3, U4 | `bot/src/user-config-loader.ts` |
| `sanitizeUsername(username)` | U1, U2, U3, U4 | `bot/src/user-config-loader.ts` |

## 决策日志

| 问题 | 决策 | 理由 |
|------|------|------|
| 用户配置存储在哪？ | EFS `knowledge/_users/` | 复用现有 EFS 架构和 `_tenants/` 模式，无需引入新存储 |
| ALB 粘性类型？ | Duration-based cookie (AWSALB) | 最简单，CDK 一行配置；Application cookie 需要改应用代码 |
| 粘性持续时间？ | 1 天 (86400 秒) | 覆盖一个工作日；Session TTL 只有 4h，1 天足够 |
| 技能合并策略？ | 全局 → 租户 → 用户 三层继承 | 用户继承租户配置，可覆盖；租户继承全局配置，可覆盖 |
| 个人调度结果发送到哪？ | 存储到 EFS 的 job 历史文件 | 无法确定用户当前在线的 IM channel，改为 Admin UI 查看历史 |
| CloudFront Cookie 转发 | 使用 CookiePolicy.ALL | ALB sticky cookie 必须通过 CF 传递给客户端 |

## 备选方案对比

| 方案 | 结论 | 理由 |
|------|------|------|
| Redis 替代 ALB Sticky | **否决** | 引入新基础设施，当前规模不需要；ALB cookie 足够 |
| DynamoDB 存用户配置 | **否决** | 增加复杂性和成本；EFS 已够用且文件系统操作更自然 |
| Application-level sticky | **否决** | 需要应用代码管理 cookie 路由，ALB 原生支持更简单 |
| EFS `_users/` 目录结构 | **选定** | 复用 `_tenants/` 模式，用户熟悉，无迁移成本 |
| ALB Cookie-based sticky | **选定** | CDK 原生支持，无需应用代码改动，CloudFront 转发即可 |
