# 构建日志: Personal Knowledge/Skills/调度 + ALB Sticky Session

## 摘要
- 日期: 2026-03-28
- 单元: 5 (U0-U4)，2 批 (Batch 1: U0+U1, Batch 2: U2+U3+U4)
- 测试: 64 通过，0 失败
- Spec 审查: PASS
- Quality 审查: PASS (附 3 条建议，已修复 1 条)

## 构建内容

### Batch 1: 基础设施 + 用户配置基座
- **U0: ALB Sticky Session** — CDK Target Group 添加 1 天粘性 Cookie + CloudFront OriginRequestPolicy 转发所有 Cookie
- **U1: User Config Loader** — `user-config-loader.ts`: sanitizeUsername (正则白名单 + 保留名), ensureUserDir, getUserKnowledgeDir, getUserConfigPath

### Batch 2: 个人化三件套 (并行构建)
- **U2: Personal Knowledge** — `personal-knowledge.ts`: scanMergedKnowledge 三层合并 (global → tenant → user, Map 覆盖) + system-prompt-builder.ts 集成
- **U3: Personal Skills** — `personal-skills.ts`: loadMergedSkills 三层技能合并 + loadUserSkillsConfig (overrides + custom_skills YAML)
- **U4: Personal Scheduler** — `personal-scheduler.ts`: loadUserScheduledJobs + saveJobResult (单调时间戳) + cleanupOldResults (默认保留 10 条)

## 遇到的问题
- Jest 配置 roots 指向 `tests/`，实际单元测试在 `bot/src/__tests__/` 使用 `node:test`。正确的运行命令是 `npx tsx --test`
- 上次构建日志的经验：worktree hooks 不兼容，本次直接主分支构建

## 新增文件

| 文件 | 行数 | 用途 |
|------|------|------|
| `bot/src/user-config-loader.ts` | 52 | 用户配置目录管理 |
| `bot/src/personal-knowledge.ts` | 59 | 三层知识合并 |
| `bot/src/personal-skills.ts` | 114 | 三层技能合并 |
| `bot/src/personal-scheduler.ts` | 137 | 用户定时任务 + 结果存储 |
| `bot/src/__tests__/user-config-loader.test.ts` | 135 | User Config Loader 测试 (14) |
| `bot/src/__tests__/personal-knowledge.test.ts` | 212 | Personal Knowledge 测试 (13) |
| `bot/src/__tests__/personal-skills.test.ts` | 397 | Personal Skills 测试 (21) |
| `bot/src/__tests__/personal-scheduler.test.ts` | 268 | Personal Scheduler 测试 (16) |

## 修改文件

| 文件 | 变更 |
|------|------|
| `infra/lib/ops-agent-stack.ts` | +stickinessCookieDuration, +OriginRequestPolicy (Cookie 转发) |
| `bot/src/system-prompt-builder.ts` | +username 字段, 改用 scanMergedKnowledge |

## 决策日志

| 决策 | 理由 |
|------|------|
| 不使用 worktree 隔离 | 上次构建经验: hooks 不兼容 |
| uniqueTimestamp() 单调递增 | 避免同毫秒内多次 saveJobResult 的文件名碰撞 |
| CF Cookie 转发 all() | 简单可靠，包括 AWSALB + opsagent_session cookie |
| 防御性 sanitizeUsername 在 saveJobResult/cleanupOldResults | 审查建议，防止路径穿越（即使上层已校验） |

## 审查结果

### Spec 审查: PASS
- 5 个单元全部符合设计规格
- Interface Contracts 匹配
- Error/Rescue Map 行为正确

### Quality 审查: PASS
建议:
1. personal-knowledge.ts getUserKnowledgeDir 返回值可复用 — 可优化，非阻断
2. saveJobResult/cleanupOldResults 需 sanitizeUsername 防御性校验 — **已修复**
3. CF Cookie 转发范围较宽，日后可考虑收窄 — 记录，当前可接受
