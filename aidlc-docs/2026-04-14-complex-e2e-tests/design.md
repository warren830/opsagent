# Design: Complex E2E Test Suite

## Architecture

```
e2e/
├── playwright.config.ts          # 已有，不修改
├── helpers.ts                    # NEW: 共享工具（login, spaNav, apiClient）
├── complex-e2e.spec.ts           # NEW: 主测试文件（4 phases）
├── full-test.spec.ts             # 已有，保留
└── auth-debug.spec.ts            # 已有，保留
```

单文件 `complex-e2e.spec.ts` 包含 4 个有序 describe blocks：
1. `Phase 1: Environment Setup` — API 创建多租户环境
2. `Phase 2: UI Business Logic` — 多角色 UI 验证
3. `Phase 3: API Security Audit` — 绕过 UI 的越权测试
4. `Phase 4: Teardown` — 清理所有测试数据

## Test Data Model

```
super_admin (admin/admin123, 已有)
  │
  ├── Tenant Alpha: "e2e-alpha-{RUN_ID}"
  │   ├── User Alpha-Admin:    member + account admin grant
  │   ├── User Alpha-Readonly: member + account readonly grant
  │   ├── Account Alpha: AWS (e2e-aws-alpha, is_mock=true)
  │   ├── Glossary: "e2e-term-alpha"
  │   └── Cluster: (static, e2e-cluster-alpha)
  │
  ├── Tenant Beta: "e2e-beta-{RUN_ID}"
  │   ├── User Beta-Admin: member + account admin grant
  │   ├── Account Beta: AWS (e2e-aws-beta, is_mock=true)
  │   └── Glossary: "e2e-term-beta"
  │
  └── Real AWS Account: 612674025488 (for discover tests, NOT created by test)
```

`RUN_ID` = `Date.now()` 的后 6 位，确保唯一性。

## Shared Helpers (helpers.ts)

```typescript
// API client: 直接 fetch 调后端 API，用 token auth
class ApiClient {
  constructor(private baseUrl: string, private token?: string)
  async get<T>(path: string): Promise<T>
  async post<T>(path: string, body?: unknown): Promise<T>
  async put<T>(path: string, body?: unknown): Promise<T>
  async del(path: string): Promise<Response>
  async login(username: string, password: string): Promise<string> // returns token
}

// Browser login: 通过 UI 登录并返回 authenticated context
async function loginAsUser(browser: Browser, username: string, password: string): Promise<{ context: BrowserContext, page: Page }>

// SPA navigation: 点击侧边栏链接
async function spaNav(page: Page, textPattern: RegExp): Promise<boolean>
```

## Phase 1: API Setup (~10 tests)

全部通过 `ApiClient` 直接调 API，不经过 UI：

1. Login as super_admin → get token
2. Create Tenant Alpha → save id
3. Create Tenant Beta → save id
4. Create User alpha-admin (member, tenant_alpha) → save id
5. Create User alpha-readonly (member, tenant_alpha) → save id
6. Create User beta-admin (member, tenant_beta) → save id
7. Create Account Alpha (aws, mock, tenant_alpha) → save id
8. Create Account Beta (aws, mock, tenant_beta) → save id
9. Grant alpha-admin → account_alpha (role=admin)
10. Grant alpha-readonly → account_alpha (role=readonly)
11. Grant beta-admin → account_beta (role=admin)

## Phase 2: UI Business Logic (~20 tests)

每个角色使用独立的 `BrowserContext`（独立 cookie jar）：

**Context A (super_admin):**
- Dashboard 显示所有 tenant 数据
- 所有页面可见

**Context B (alpha-admin):**
- CRUD Glossary: create → edit → verify → delete
- CRUD Cluster (static): create → verify
- Accounts 页面只显示被授权的 account
- 看不到 Beta tenant 的数据

**Context C (alpha-readonly):**
- 所有列表页可以查看
- 创建/编辑/删除按钮被禁用或隐藏
- 直接尝试 API 写操作返回 403

**Context D (beta-admin):**
- 看不到 Alpha tenant 的数据
- 只看到 Beta 的 account

**Real AWS tests (super_admin context):**
- Discover clusters: POST /api/clusters/discover → at least 1 cluster
- Topology page loads with discovered cluster data

## Phase 3: API Security Audit (~15 tests)

直接用 `fetch` + token 调用 API，不通过 UI：

**401 Tests (无 token):**
- GET /api/tenants → 401
- GET /api/users → 401
- POST /api/accounts → 401

**403 Tests (member 调 super_admin 端点):**
- alpha-admin POST /api/tenants → 403
- alpha-admin POST /api/users → 403
- alpha-admin DELETE /api/tenants/{id} → 403

**跨租户越权:**
- alpha-admin GET /api/glossary → 不包含 beta 的数据
- alpha-admin PUT /api/accounts/{beta_account_id} → 403
- alpha-admin DELETE /api/accounts/{beta_account_id} → 403

**Readonly 写拦截:**
- alpha-readonly POST /api/glossary → 403
- alpha-readonly PUT /api/glossary/{id} → 403
- alpha-readonly DELETE /api/glossary/{id} → 403

## Phase 4: Teardown (~5 tests)

按依赖逆序删除（用 super_admin token）：
1. Delete account access grants
2. Delete glossary entries (if any remain)
3. Delete clusters (if any remain)
4. Delete accounts
5. Delete users
6. Delete tenants
7. Verify: GET /api/tenants 不包含 e2e- 前缀的数据

## Decisions Log

| Decision | Choice | Reason |
|----------|--------|--------|
| tenant_admin 角色 | 用 member + admin grant 替代 | API 不支持创建 tenant_admin |
| 测试账号类型 | is_mock=true | 避免影响真实 AWS 资源 |
| RUN_ID | Date.now() 后 6 位 | 够唯一且可读 |
| 文件组织 | 单文件 + helpers | 保持简单，Phase 间需要共享状态 |
| Real AWS discover | 放在 Phase 2 super_admin 下 | 依赖真实 AWS 连接，可能 flaky |
