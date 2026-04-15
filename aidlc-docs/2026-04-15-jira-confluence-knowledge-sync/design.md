# Design: Jira/Confluence → Knowledge Base 同步

## 概述

通过扩展现有 Jira client + Knowledge 模块，实现从 Jira Issues 和 Confluence Pages 增量同步内容到 Knowledge Base，支持手动触发 + 定时自动同步。

## 用户决策

| 决策 | 选项 | 结果 |
|------|------|------|
| 数据源 | Jira + Confluence | 两者都要 |
| 同步方式 | 手动 + 定时 | 首次手动，之后定时增量 |
| 同步粒度 | 增量更新 | 只同步新增/修改，保留本地编辑 |

## 架构设计

### 数据流

```
Jira Issues ──┐
              ├──→ Sync Service ──→ knowledge_files (source='jira'/'confluence')
Confluence ───┘         ↑
                        │
              Scheduled Job (cron) 或 手动触发
```

### 核心改动

**1. 扩展 knowledge_files 表** — 新增 migration 加字段追踪同步来源：
```sql
ALTER TABLE knowledge_files ADD COLUMN source VARCHAR(20) DEFAULT 'manual';
ALTER TABLE knowledge_files ADD COLUMN source_id VARCHAR(255);  -- Jira issue key / Confluence page id
ALTER TABLE knowledge_files ADD COLUMN source_url TEXT;          -- 原始链接
ALTER TABLE knowledge_files ADD COLUMN source_updated_at TIMESTAMPTZ; -- 源端最后更新时间（增量用）
CREATE UNIQUE INDEX idx_knowledge_source ON knowledge_files(source, source_id) WHERE source_id IS NOT NULL;
```

**2. 扩展 Jira client** — 添加搜索和 Confluence 方法：
- `search_issues(jql, max_results)` — JQL 搜索 Jira issues
- `get_confluence_pages(space_key, limit)` — 获取 Confluence space 下的页面列表
- `get_confluence_page(page_id)` — 获取单个页面内容（转为 Markdown）

**3. 新建 Sync Service** — `services/knowledge_sync.rs`：
- `sync_jira(pool, auth_user, channel, jql_filter)` — 从 Jira 同步
- `sync_confluence(pool, auth_user, channel, space_key)` — 从 Confluence 同步
- 增量逻辑：比较 `source_updated_at` vs 远端 `updated`，只更新有变化的

**4. 新建 API 端点**：
- `POST /api/knowledge/sync` — 手动触发同步 `{ source: "jira"|"confluence", channel_id, filter }`
- `GET /api/knowledge/sync/status` — 查看最近同步状态

**5. 前端 Knowledge 页面** — 添加同步按钮和配置 dialog

### 不改的部分
- Channel CRUD — 复用，用户在 Channels 页面配置 Jira/Confluence 连接
- Knowledge CRUD — 复用，同步的文件和手动上传的文件共存
- Scheduled Jobs — 复用，用户创建定时同步任务

## 构建单元

| # | 单元 | 类型 | 依赖 |
|---|------|------|------|
| U1 | Migration: knowledge_files 加 source 字段 | Backend | 无 |
| U2 | 扩展 Jira client: search_issues + Confluence API | Backend | 无 |
| U3 | Sync Service: sync_jira + sync_confluence | Backend | U1, U2 |
| U4 | API endpoints: POST /api/knowledge/sync | Backend | U3 |
| U5 | 前端 Knowledge 页面: 同步按钮 + dialog | Frontend | U4 |

U1 和 U2 可并行，U3 依赖 U1+U2，之后 U4 → U5 串行。
