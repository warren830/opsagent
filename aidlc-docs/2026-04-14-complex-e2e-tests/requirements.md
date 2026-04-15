# Requirements: Complex E2E Tests — Multi-Tenant RBAC & Business Logic

## Problem Statement

现有 E2E 测试只覆盖"页面能不能打开"和"按钮能不能点"，完全没有测试多租户数据隔离、RBAC 权限控制、CRUD 全生命周期、真实 AWS 集成等核心业务逻辑。后端有 331 处 `tenant_id` 引用、51 个文件涉及租户隔离逻辑，但没有一个 E2E 测试验证过这些。需要一套能自建完整多租户环境、覆盖权限矩阵、包含 API 安全审计的复杂测试套件。

## Chosen Approach: 混合策略 (API Setup + UI Verify + API Security Audit)

**理由**: API 层负责快速搭建多租户环境（创建 tenant/user/account 比 UI 快 10 倍）和安全审计（绕过 UI 直接用错误 token 打 API 验证后端防线）。UI 层负责验证关键业务流程和 UX 体验。两层互补，兼顾速度、稳定性和覆盖率。

## Multi-Perspective Review

### Product
- 多租户 RBAC 隔离是平台核心安全特性，一个 IDOR 漏洞就是 P0 数据泄露
- API 层安全审计 ROI 极高：UI 隐藏按钮不代表后端拒绝了请求
- 80/20 的 5 个必测场景：(1) 跨租户隔离 (2) Accounts CRUD + discover (3) Clusters CRUD + EKS discover (4) Readonly 写操作拦截 (5) 认证生命周期
- Chat/AI Agent 的租户隔离是高风险盲区，后续应补充

### Engineering
- 每个用户角色使用独立的 `browser.newContext()`，不要在同一个 context 中 logout/login
- Real AWS discovery 是最大 flaky 源：需要长 timeout、`at least N` 断言、独立为可选 suite
- 必须 `workers: 1` 串行执行，所有测试共享同一个后端状态
- 所有测试数据用 `e2e-{run_id}` 前缀，teardown 阶段强制清理

### Design
- Readonly 用户写操作被拒绝时必须有友好提示（toast/禁用态），不能是空白 500 错误
- Cluster Discover、Account Sync 是异步操作，需要测 loading 中间态和超时恢复
- 级联删除（删 Tenant → 影响 Users/Accounts）必须测确认弹窗是否告知影响范围
- 表单边界：长名称截断、特殊字符、重复名称的冲突提示

## Scope

### In Scope
- 测试脚本自建完整多租户环境（2 tenants, 4+ users, multiple accounts）
- 全 CRUD 生命周期（创建 → 读取 → 编辑 → 删除 → 验证清理）on: Tenants, Users, Accounts, Clusters, Glossary, Knowledge, Skills, Channels, Telemetry, Scheduled Jobs, Providers, MCP Servers, Code Repos
- RBAC 权限矩阵验证（super_admin / member+admin_grant / member+readonly_grant）
- 跨租户数据隔离验证（Tenant A 看不到 Tenant B 的任何资源）
- API 安全审计（用 Tenant A 的 token 直接打 Tenant B 的 API）
- 真实 AWS 集成（Discover Accounts, Discover EKS Clusters）
- 每个关键操作截图
- Teardown 清理所有测试数据

### Out of Scope
- Chat/AI Agent 集成测试（后续单独做）
- 破坏性 AWS 操作（不删除真实云资源）
- 性能/压力测试
- OAuth 登录流程（Microsoft/Cognito）
- Rollout promote/rollback（需要真实 Argo Rollouts 环境）

## Test Architecture

### Phase 1: API Setup (快速, ~10s)
通过 API 搭建完整多租户环境：
```
super_admin (已有: admin/admin123)
  ├── Tenant A: "e2e-tenant-alpha-{run_id}"
  │   ├── User A-Admin: member + account admin grant
  │   ├── User A-Readonly: member + account readonly grant
  │   └── Account A: AWS account (e2e-account-alpha)
  ├── Tenant B: "e2e-tenant-beta-{run_id}"
  │   ├── User B-Admin: member + account admin grant
  │   └── Account B: AWS account (e2e-account-beta)
  └── Shared: Real AWS account (612674025488) for discovery tests
```

### Phase 2: UI Business Logic Tests (via SPA navigation)
每个用户角色登录独立 BrowserContext，验证：
1. **Dashboard 数据隔离**: Tenant A 的 admin 只看到自己的统计
2. **CRUD 全流程**: 在 Accounts/Clusters/Glossary/Skills 等页面完成创建→编辑→删除
3. **Readonly 拒绝验证**: Readonly 用户看到禁用的操作按钮，提交被拒绝
4. **跨租户不可见**: Tenant A 的资源列表不包含 Tenant B 的数据
5. **真实 AWS Discover**: 触发 Account Discover + Cluster Discover，验证结果

### Phase 3: API Security Audit (绕过 UI)
直接用 fetch 调用 API，验证后端权限：
1. **401 Unauthorized**: 无 token 访问所有 protected 端点
2. **403 Forbidden**: member 调用 super_admin 端点（create tenant, create user）
3. **跨租户越权**: Tenant A 的 token 访问 Tenant B 的资源
4. **Readonly 写拦截**: readonly 用户调用写 API（POST/PUT/DELETE）
5. **Account 越权**: 无 account access 的用户调用 account-bound 资源

### Phase 4: Teardown (清理)
按依赖逆序删除所有测试数据：
Account Access Grants → Accounts → Users → Tenants

## Requirements (Specific & Testable)

### 环境搭建
1. 通过 API 创建 2 个 tenant，命名含唯一 run_id
2. 通过 API 创建 4 个 user（2 per tenant），每个 tenant 有 admin-grant 和 readonly-grant
3. 通过 API 创建 2 个 cloud account（每 tenant 1 个）
4. 通过 API 授予用户对应的 account access（admin/readonly）
5. 所有创建操作验证返回 200 并保存 ID 供后续使用

### CRUD 全生命周期 (per resource type)
6. 创建资源 → API 返回 201/200 + 资源出现在列表中
7. 编辑资源 → 修改后的字段在列表中更新
8. 删除资源 → 确认弹窗 → 资源从列表消失
9. 空态展示 → 删除最后一条后显示 "No data" 或空态占位

### RBAC 权限
10. super_admin 能看到所有 tenant 的所有资源
11. member(admin grant) 只能看到自己 tenant + 被授权的 account 的资源
12. member(readonly grant) 能看但不能改（UI 按钮禁用/隐藏 + API 返回 403）
13. member 不能创建 tenant 或 user（API 返回 403）

### 多租户隔离
14. GET /api/tenants — member 只返回自己的 tenant
15. GET /api/accounts — member 只返回被授权的 accounts
16. GET /api/clusters — 按 tenant 过滤
17. GET /api/glossary — 按 accessible accounts 过滤
18. GET /api/issues — 按 tenant 过滤
19. Tenant A 的 token 调用 PUT/DELETE /api/accounts/{tenant_b_account_id} → 403

### 真实 AWS 集成
20. POST /api/accounts/{id}/test — 真实 AWS 账号返回连接成功
21. POST /api/clusters/discover — 发现至少 1 个 EKS 集群
22. GET /api/clusters 列表包含发现的集群

### 安全审计 (API-level)
23. 所有 protected 端点无 token → 401
24. member 调用 POST /api/tenants → 403
25. member 调用 POST /api/users → 403
26. Tenant A token 调用 GET /api/glossary 不返回 Tenant B 的数据
27. Readonly 调用 POST/PUT/DELETE glossary/knowledge → 403

## Success Criteria
- 所有测试用例通过
- 每个关键操作有截图证据
- API 安全审计发现 0 个越权漏洞
- 测试数据在 teardown 后完全清理
- 测试总时间 < 5 分钟

## User Flows (Critical 5)

1. **Super Admin → 创建多租户环境**: Login → Create Tenant → Create User → Create Account → Grant Access
2. **Tenant Admin → 业务操作**: Login → View Dashboard → Add Account → Discover Clusters → Create Glossary → Edit → Delete
3. **Readonly → 只读验证**: Login → View all pages → Try edit (blocked) → Try delete (blocked)
4. **跨租户隔离**: Login as Tenant A → List resources → Verify no Tenant B data → Try API cross-access (403)
5. **AWS Discovery**: Login → Cloud Accounts → Sync Organization → Clusters → Discover → Verify results

## Technical Risks

| 风险 | 影响 | 缓解 |
|------|------|------|
| AWS Discover 超时/rate limit | 测试 flaky | 独立 suite, 30s timeout, "at least N" 断言 |
| 测试中断导致数据残留 | 后续测试失败 | unique run_id + beforeAll 清理旧数据 |
| CloudFront 缓存旧页面 | 测试看到旧 UI | 测试前 invalidate 或直接打 EKS origin |
| SPA 导航限制 (page.goto 丢 auth) | 无法硬导航 | 全部用侧边栏点击导航 |
| tenant_admin 角色无法通过 API 创建 | 无法测试 tenant_admin 特有权限 | 用 member + account admin grant 模拟, 或 SQL seed |

## Assumptions
- admin/admin123 凭据在测试环境可用
- AWS 账号 612674025488 的 IAM role 有 EKS:ListClusters 和 Organizations:ListAccounts 权限
- 测试运行环境可以访问 https://dg00c54mwvycp.cloudfront.net
- 后端已部署最新的 cookie Secure + SSR 修复

## Open Questions

### Blocking (resolve before design)
- `tenant_admin` 角色能否通过 API 创建？还是只能通过 SQL？如果不能，测试如何覆盖 tenant_admin 特有权限（如 grant account access）？

### Deferrable (resolve during implementation)
- Chat session 的租户隔离如何测试？需要等 Chat 功能更稳定后单独做
- Rollout promote/rollback 测试需要真实 Argo 环境，是否需要在 EKS 上部署 demo app？
- 表单边界测试（XSS payload, SQL injection in input）是否纳入本轮？

## Research Context

### API 端点总览
后端共 100+ 个 API 端点，分布在 auth, tenants, users, accounts, account-access, clusters, rollouts, glossary, knowledge, skills, channels, telemetry, providers, mcp, chat, issues, approvals, scheduled-jobs, pipeline, resources, topology, dashboard 等模块。

### RBAC 模型
- 三层角色: super_admin (全局) > tenant_admin (租户) > member (普通用户)
- 账号级权限: admin (读写) / readonly (只读)，通过 user_account_access 表管理
- 核心函数: `get_accessible_account_ids()`, `can_write_account()`, `tenant_filter()`

### 已知 Patterns
- 所有受保护端点通过 JWT middleware 提取 AuthUser（含 user_id, role, tenant_id）
- Token 从 Cookie 或 Authorization header 提取，Cookie 优先
- 多租户过滤: super_admin 无过滤, 其他角色按 tenant_id 过滤
