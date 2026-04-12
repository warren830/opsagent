# 后端 Service 层提取 — 设计文档

**日期**: 2026-04-12
**复杂度**: Heavy
**策略**: 渐进式 TDD 重构

---

## 目标

将当前 handler 中直接嵌入的数据库操作和业务逻辑提取到独立的 service 层，实现：
1. **关注点分离** — handler 只处理 HTTP 语义（解析请求、返回响应），service 处理业务逻辑
2. **可测试性** — service 函数可独立进行集成测试
3. **代码复用** — 消除 15+ handler 中重复的租户过滤逻辑
4. **一致性** — 统一错误处理和验证模式

---

## 当前架构

```
Handler (HTTP) ──直接调用──> sqlx::query (DB)
     │
     └── 混合了：验证、权限检查、SQL 查询、错误映射
```

**问题**:
- 28 个 handler 文件中，只有少数几个有对应 service（auth_common、claude、k8s 等复杂逻辑）
- 简单 CRUD handler（tenant、user、channel、glossary 等）直接写 SQL
- `is_super_admin()` 租户过滤逻辑在 15+ 处重复
- 后台任务直接调用 handler 函数（如 `handlers::cloud_account::sync_org_accounts`）

---

## 目标架构

```
Handler (HTTP)
  │  职责：解析参数、调用 service、返回 JSON
  │
  ▼
Service (Business Logic)
  │  职责：权限检查、验证、业务规则、错误映射
  │
  ▼
Repository / sqlx queries (Data Access)
  │  职责：纯数据库操作
  └── 暂时不独立成 repository 层，service 直接用 sqlx
```

**决策**: 不引入独立 Repository 层（过度抽象），service 直接使用 `PgPool` 和 `sqlx::query`。

---

## 分批计划

### Batch 1: 基础 CRUD（模式统一）

| 模块 | 当前行数 | 说明 |
|------|---------|------|
| `tenant` | 146 行 | 最典型的 CRUD，作为模板 |
| `channel` | 130 行 | 几乎一样的模式 |
| `glossary` | ~200 行 | 稍复杂，有 bulk import |

**目标**：建立 service 层模式，后续模块照搬。

### Batch 2: 带业务逻辑的 CRUD

| 模块 | 当前行数 | 说明 |
|------|---------|------|
| `user` | 218 行 | 有 bcrypt、角色验证、invite 逻辑 |
| `knowledge` | ~150 行 | 简单 CRUD |
| `scheduled_job` | ~300 行 | 有 cron 触发逻辑 |

### Batch 3: 复杂模块

| 模块 | 当前行数 | 说明 |
|------|---------|------|
| `cloud_account` | ~400 行 | 有 AWS 发现、org sync、后台任务 |
| `cluster` | ~300 行 | 有 K8s 发现逻辑 |
| `approval` | ~200 行 | 审批状态机 |

### Batch 4: 后台任务统一

将 `main.rs` 中 6 个 `tokio::spawn` 提取到 `services/background.rs` 统一管理。

---

## Service 层接口设计

### 通用模式

```rust
// services/tenant.rs (新建)

use sqlx::PgPool;
use uuid::Uuid;
use crate::error::{AppError, AppResult};
use crate::middleware::auth::AuthUser;
use crate::models::tenant::{CreateTenantRequest, Tenant, UpdateTenantRequest};

/// List tenants visible to the authenticated user.
pub async fn list(pool: &PgPool, auth_user: &AuthUser) -> AppResult<Vec<Tenant>> {
    if auth_user.is_super_admin() {
        Ok(sqlx::query_as::<_, Tenant>("SELECT * FROM tenants ORDER BY name")
            .fetch_all(pool)
            .await?)
    } else {
        match auth_user.tenant_id {
            Some(tid) => Ok(sqlx::query_as::<_, Tenant>("SELECT * FROM tenants WHERE id = $1")
                .bind(tid)
                .fetch_all(pool)
                .await?),
            None => Ok(vec![]),
        }
    }
}

/// Create a new tenant (super_admin only).
pub async fn create(pool: &PgPool, auth_user: &AuthUser, req: &CreateTenantRequest) -> AppResult<Tenant> {
    if !auth_user.is_super_admin() {
        return Err(AppError::Forbidden("Only super admins can create tenants".into()));
    }
    validate_create(req)?;
    // ... DB insert with constraint error mapping
}

/// Get tenant by ID with access check.
pub async fn get(pool: &PgPool, auth_user: &AuthUser, id: Uuid) -> AppResult<Tenant> { ... }

/// Update tenant (super_admin only).
pub async fn update(pool: &PgPool, auth_user: &AuthUser, id: Uuid, req: &UpdateTenantRequest) -> AppResult<Tenant> { ... }

/// Delete tenant (super_admin only).
pub async fn delete(pool: &PgPool, auth_user: &AuthUser, id: Uuid) -> AppResult<()> { ... }
```

### 重构后的 Handler

```rust
// handlers/tenant.rs (重构后)

/// GET /api/tenants
pub async fn list_tenants(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
) -> AppResult<Json<Vec<Tenant>>> {
    let tenants = services::tenant::list(&state.pool, &auth_user).await?;
    Ok(Json(tenants))
}

/// POST /api/tenants
pub async fn create_tenant(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Json(req): Json<CreateTenantRequest>,
) -> AppResult<Json<Tenant>> {
    let tenant = services::tenant::create(&state.pool, &auth_user, &req).await?;
    Ok(Json(tenant))
}
```

---

## 共享工具函数

### 租户过滤 helper

```rust
// services/common.rs (新建)

use crate::middleware::auth::AuthUser;

/// Build a tenant filter clause for queries.
/// Returns (where_clause, bind_value) pair.
pub fn tenant_filter(auth_user: &AuthUser) -> Option<Uuid> {
    if auth_user.is_super_admin() {
        None // no filter
    } else {
        auth_user.tenant_id
    }
}
```

### 数据库约束错误映射

```rust
// services/common.rs

use crate::error::AppError;

/// Map database constraint violations to user-friendly errors.
pub fn map_constraint_error(e: sqlx::Error, mappings: &[(&str, &str)]) -> AppError {
    if let sqlx::Error::Database(ref db_err) = e {
        if let Some(constraint) = db_err.constraint() {
            for (name, message) in mappings {
                if constraint == *name {
                    return AppError::Conflict(message.to_string());
                }
            }
        }
    }
    AppError::Database(e)
}
```

---

## 测试策略

### 测试基础设施

使用 `sqlx::test` 宏进行集成测试，每个测试自动获得一个隔离的数据库事务。

```rust
// tests/services/tenant_test.rs

#[sqlx::test(migrations = "src/migrations")]
async fn test_list_tenants_super_admin(pool: PgPool) {
    // Arrange: create test tenant
    let tenant = seed_tenant(&pool, "test-tenant").await;
    let admin = fake_super_admin();

    // Act
    let result = services::tenant::list(&pool, &admin).await.unwrap();

    // Assert
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].name, "test-tenant");
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_list_tenants_member_sees_own_only(pool: PgPool) {
    let t1 = seed_tenant(&pool, "tenant-1").await;
    let t2 = seed_tenant(&pool, "tenant-2").await;
    let member = fake_member(t1.id);

    let result = services::tenant::list(&pool, &member).await.unwrap();

    assert_eq!(result.len(), 1);
    assert_eq!(result[0].id, t1.id);
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_create_tenant_non_admin_forbidden(pool: PgPool) {
    let member = fake_member(Uuid::new_v4());
    let req = CreateTenantRequest { name: "x".into(), slug: "x".into(), .. };

    let result = services::tenant::create(&pool, &member, &req).await;

    assert!(matches!(result, Err(AppError::Forbidden(_))));
}
```

### 测试 helper 模块

```rust
// tests/helpers/mod.rs

pub fn fake_super_admin() -> AuthUser {
    AuthUser { user_id: Uuid::new_v4(), role: "super_admin".into(), tenant_id: None }
}

pub fn fake_member(tenant_id: Uuid) -> AuthUser {
    AuthUser { user_id: Uuid::new_v4(), role: "member".into(), tenant_id: Some(tenant_id) }
}

pub async fn seed_tenant(pool: &PgPool, name: &str) -> Tenant {
    sqlx::query_as::<_, Tenant>(
        "INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING *"
    )
    .bind(name)
    .bind(name)
    .fetch_one(pool)
    .await
    .unwrap()
}
```

---

## 文件结构变更

```
backend/src/
├── services/
│   ├── mod.rs              # 更新：添加新模块
│   ├── common.rs           # 新建：共享工具（tenant_filter、map_constraint_error）
│   ├── tenant.rs           # 新建：Batch 1
│   ├── channel.rs          # 新建：Batch 1
│   ├── glossary.rs         # 新建：Batch 1
│   ├── user.rs             # 新建：Batch 2
│   ├── knowledge.rs        # 新建：Batch 2
│   ├── scheduled_job.rs    # 新建：Batch 2
│   ├── cloud_account.rs    # 新建：Batch 3
│   ├── cluster_mgmt.rs     # 新建：Batch 3
│   ├── approval.rs         # 新建：Batch 3
│   ├── background.rs       # 新建：Batch 4（后台任务管理）
│   ├── ... (existing)
│
├── tests/                  # 新建目录
│   ├── helpers/
│   │   └── mod.rs          # 测试工具函数
│   └── services/
│       ├── mod.rs
│       ├── tenant_test.rs
│       ├── channel_test.rs
│       └── ...
```

---

## 决策日志

| 决策 | 选择 | 理由 |
|------|------|------|
| 是否引入 Repository 层 | 否 | 项目规模不大，service 直接用 sqlx 足够。避免过度抽象。 |
| 是否引入 trait 做 DI | 否 | 使用 `sqlx::test` 已有隔离事务，不需要 mock DB。 |
| 测试类型 | 集成测试（sqlx::test） | Rust 类型系统保证编译正确性，集成测试验证 SQL 正确性更有价值。 |
| 是否用宏统一 CRUD | 暂不 | 先手工提取看模式，后续可考虑。避免过早抽象。 |
| Handler 中是否保留任何逻辑 | 仅 HTTP 语义 | 参数解析、响应包装留在 handler，所有业务逻辑移到 service。 |
| Batch 1 选哪几个模块 | tenant + channel + glossary | 最简单的 CRUD，建立模式后其他照搬。 |

---

## 风险与缓解

| 风险 | 影响 | 缓解措施 |
|------|------|---------|
| 大量文件变更导致合并冲突 | 中 | 每个 batch 一个 PR，及时合并 |
| SQL 在迁移过程中行为变化 | 低 | 集成测试覆盖所有路径 |
| `main.rs` 后台任务依赖 handler 函数 | 中 | Batch 3 中将 `sync_org_accounts` 等移到 service |
| 测试需要 PostgreSQL | 低 | 使用 `docker compose` 中已有的 PG |

---

## 验收标准

- [ ] Batch 1 完成后：`tenant`、`channel`、`glossary` handler 中无直接 SQL
- [ ] 每个 service 函数有至少 1 个集成测试
- [ ] `cargo build` 通过
- [ ] `cargo clippy` 无警告
- [ ] `cargo test` 全部通过
- [ ] Handler 代码行数减少 50%+（逻辑移到 service）
