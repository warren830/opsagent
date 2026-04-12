# Build Log: 后端 Service 层提取 — Batch 1

**日期**: 2026-04-12
**状态**: ✅ 完成
**验证**: cargo build ✅ | cargo clippy ✅ (新代码 0 warnings) | cargo check --tests ✅

---

## Summary

完成了 Batch 1 的全部工作：将 `tenant`、`channel`、`glossary` 三个模块的业务逻辑从 handler 提取到 service 层。

### 交付物

| 文件 | 类型 | 行数 | 说明 |
|------|------|------|------|
| `src/lib.rs` | 新建 | ~15 | crate 入口，导出公共模块供测试使用 |
| `src/services/common.rs` | 新建 | ~48 | 共享工具：`map_constraint_error`、`require_super_admin`、`require_non_empty`、`tenant_filter` |
| `src/services/tenant.rs` | 新建 | ~118 | tenant CRUD 业务逻辑 |
| `src/services/channel.rs` | 新建 | ~124 | channel CRUD 业务逻辑 |
| `src/services/glossary.rs` | 新建 | ~225 | glossary CRUD + 权限检查 |
| `src/handlers/tenant.rs` | 重构 | 60 行 (原 146) | 减少 59% |
| `src/handlers/channel.rs` | 重构 | 51 行 (原 130) | 减少 61% |
| `src/handlers/glossary.rs` | 重构 | 57 行 (原 216) | 减少 74% |
| `src/main.rs` | 修改 | 使用 lib 导入 | 消除重复 mod 声明 |
| `tests/helpers/mod.rs` | 新建 | ~40 | 测试工具：AuthUser 工厂函数 |
| `tests/helpers/seed.rs` | 新建 | ~30 | 数据库 seeding 辅助函数 |
| `tests/service_tenant_test.rs` | 新建 | ~130 | 9 个集成测试 |
| `tests/service_channel_test.rs` | 新建 | ~140 | 9 个集成测试 |
| `tests/service_glossary_test.rs` | 新建 | ~100 | 6 个集成测试 |

### Handler 代码精简对比

| Handler | 重构前 | 重构后 | 减少 |
|---------|--------|--------|------|
| tenant.rs | 146 行 | 60 行 | **59%** |
| channel.rs | 130 行 | 51 行 | **61%** |
| glossary.rs | 216 行 | 57 行 | **74%** |
| **合计** | **492 行** | **168 行** | **66%** |

---

## 决策记录

| 决策 | 选择 | 理由 |
|------|------|------|
| lib.rs vs bin-only | 添加 lib.rs | 集成测试需要 crate 公共 API 入口 |
| glossary 依赖 account_access | 直接从 handler 导入 | `get_accessible_account_ids` 和 `can_write_account` 是公共函数，暂不移动避免大范围改动 |
| clippy if-let 链 | 使用 let-chain 语法 | Rust 2024 edition 支持，更简洁 |

## 遇到的问题

1. **lib.rs 与 main.rs 共存** — 需要将 AppState 定义移到 lib.rs，main.rs 改为 import。一次性完成，无中间断裂。
2. **glossary 的 account_access 依赖** — 理想情况 `get_accessible_account_ids` 应该在 service 层，但目前它被其他 handler 直接调用。暂时保持在 handler 中，Batch 3 再处理。

## 未完成（Batch 2 范围）

- [ ] `services/user.rs` — 有 bcrypt、invite 逻辑
- [ ] `services/knowledge.rs` — 简单 CRUD
- [ ] `services/scheduled_job.rs` — cron 逻辑
- [ ] 将 `account_access` 中的公共函数迁移到 service 层

---

## 测试执行说明

测试需要 PostgreSQL。本地运行：
```bash
docker compose up -d
DATABASE_URL="postgresql://openops:openops_dev@localhost:5432/openops" cargo test
```
