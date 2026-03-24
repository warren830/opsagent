# 构建日志: AIOps 平台演进

## 摘要
- 日期: 2026-03-24 ~ 2026-03-25
- 单元: 7 (U0-U6)，3 批 (Batch 1: U0+U1+U2, Batch 2: U3+U4+U5, Batch 3: U6)
- 测试: 174 通过，0 失败
- 代码审查: TDD 流程（RED → GREEN → REFACTOR）
- 质量审查: TypeScript strict 编译 + 全量测试

## 构建内容

### Batch 1: 基础 + 扫描
- **U0: 数据库层** — PostgreSQL 客户端 (pg), 4 表 schema, 类型导出
- **U1: 增强工具集** — 22 个专用云工具 (EC2/RDS/VPC/CloudWatch/CloudTrail 等)
- **U2: 资源扫描** — scan_resources + query_resources DB 工具

### Batch 2: 巡检 + RCA
- **U3: 健康巡检** — Issue 生命周期, 指纹去重, 巡检 prompt, DB 持久化
- **U4: RCA 引擎** — 8 步调查协议, 置信度评分, 证据收集, DB 持久化
- **U5: 告警 Webhook** — CloudWatch/Datadog/通用 webhook 解析, 自动 RCA 触发

### Batch 3: UI
- **U6: Issues + Resources UI** — Issues 列表/详情/RCA 查看, Resources 搜索/KPI

## 遇到的问题
- Git worktree hooks 不兼容，改为主分支直接构建
- `mock.module` 在 ts-node 下不工作，改用直接导入 + isHealthy() 测试真实连接失败
- `require('./db')` 的 `queryOne<T>` 泛型不兼容，改用 `as typeof import('./db')` 类型断言
- E2E 测试 sidebar nav link 数量需要随 tab 增加而更新

## 时间分布
- Inception: ~30 min (问题 + 设计文档)
- Batch 1 构建: ~45 min (U0: 15min, U1+U2: 30min)
- Batch 2 构建: ~40 min (U3: 15min, U4: 12min, U5: 13min)
- Batch 3 构建: ~30 min (API + UI)
- 验证: ~15 min (编译 + 测试 + E2E)

## 新增文件

| 文件 | 行数 | 用途 |
|------|------|------|
| `bot/src/db.ts` | 205 | PostgreSQL 客户端 |
| `bot/src/enriched-tools.ts` | 335 | 22 个增强云工具 |
| `bot/src/patrol.ts` | 222 | 健康巡检引擎 |
| `bot/src/rca.ts` | 182 | RCA 根因分析引擎 |
| `bot/src/alert-webhook.ts` | 159 | 告警 Webhook 解析 |
| `bot/src/__tests__/db.test.ts` | 83 | DB 测试 |
| `bot/src/__tests__/enriched-tools.test.ts` | 108 | 增强工具测试 |
| `bot/src/__tests__/patrol.test.ts` | 113 | 巡检测试 |
| `bot/src/__tests__/rca.test.ts` | 101 | RCA 测试 |
| `bot/src/__tests__/alert-webhook.test.ts` | 107 | Webhook 测试 |
| `bot/src/__tests__/issues-api.test.ts` | 78 | 集成测试 |
| **总新增** | **~1693** | |

## 审批
- 设计审批: 2026-03-24 (用户确认)
- 安全基线: 已启用
