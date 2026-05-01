# Ops 本地开发指南

<p align="right"><a href="local-development.md">English</a> · <strong>中文</strong></p>

## 前置依赖

| 工具 | 最低版本 | 检查命令 | 安装 |
|---|---|---|---|
| Docker | 20+（含 Compose V2） | `docker --version` | [docker.com](https://docs.docker.com/get-docker/) |
| Rust | 1.83+ | `rustc --version` | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| Node.js | 20+ | `node --version` | [nodejs.org](https://nodejs.org/) 或 `brew install node` |
| npm | 10+ | `npm --version` | 随 Node.js 附带 |

> **macOS 用户**：`brew install rustup node docker` 一条命令装齐。

---

## 快速开始（单条命令）

```bash
./scripts/local-dev.sh
```

脚本会做这些事：
1. 检查前置依赖是否都装好
2. 把 `.env.example` 复制成 `.env`（若不存在）
3. 通过 Docker Compose 启动 PostgreSQL
4. 等待数据库就绪
5. 在 `:3080` 启动 Rust 后端
6. 在 `:3000` 启动 Nuxt 前端

全部跑起来后：

```
前端:    http://localhost:3003
后端:    http://localhost:8003
健康检查: http://localhost:8003/health
登录:    admin / admin123
```

按 `Ctrl+C` 停掉所有服务。

---

## 手动启动（分步）

如果你想每个组件在独立终端里跑、或者想更细控制：

### 第 1 步：环境变量

```bash
cp .env.example .env
```

默认值开箱即用。关键配置：

| 变量 | 默认值 | 说明 |
|---|---|---|
| `OPS_ENV` | `local` | 环境模式 |
| `DATABASE_URL` | `postgresql://ops:ops_dev@localhost:5432/ops` | PostgreSQL 连接串 |
| `JWT_SECRET` | `dev-secret-minimum-32-characters-long-change-in-prod` | JWT 签名密钥 |
| `BACKEND_PORT` | `3080` | Rust 服务端口 |
| `FRONTEND_PORT` | `3000` | Nuxt 开发服务器端口 |
| `ALLOWED_ORIGINS` | `http://localhost:3003` | CORS 允许来源 |
| `RUST_LOG` | `ops=debug,tower_http=debug` | 日志级别 |

### 第 2 步：启动数据库

```bash
# 只启动 PostgreSQL
docker compose up -d postgres

# 确认运行中
docker compose ps
docker compose exec postgres pg_isready -U ops
```

如果还需要 Redis（本地开发可选）：

```bash
docker compose up -d
```

### 第 3 步：启动后端（终端 1）

```bash
cd backend
cargo run
```

首次运行会：
- 下载并编译所有 Rust 依赖（约 2~5 分钟）
- 自动执行数据库迁移
- 种入默认管理员（`admin` / `admin123`）
- 在 `http://localhost:8003` 启动服务

应能看到：

```
INFO ops: Starting Ops backend (env=Local)
INFO ops::db: Database pool created (max=20, min=5)
INFO ops::db: Running database migrations...
INFO ops: Default admin user created (username: admin, password: admin123)
INFO ops: Listening on 0.0.0.0:3080
```

### 第 4 步：启动前端（终端 2）

```bash
cd frontend
npm install   # 仅首次
npm run dev
```

Nuxt 开发服务器会在 `http://localhost:3003` 启动，带：
- HMR（热模块替换）
- API 代理：`/api/**` → `http://localhost:8003/api/**`
- Vue DevTools 已启用

### 第 5 步：验证

```bash
# 健康检查
curl http://localhost:8003/health

# 登录（拿到 JWT token）
curl -s -X POST http://localhost:8003/api/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"admin123"}' | jq .

# 或者直接打开浏览器
open http://localhost:3003
```

---

## 服务架构（本地）

```
浏览器 (:3000)
    │
    ▼
┌──────────────────┐
│ Nuxt 开发服务器  │  :3000  (HMR + API 代理)
│ (frontend/)      │
└────────┬─────────┘
         │ /api/** 代理
         ▼
┌──────────────────┐
│  Axum 后端        │  :3080  (REST API)
│  (backend/)       │
└────────┬─────────┘
         │
    ┌────┴────┐
    ▼         ▼
┌────────┐ ┌───────┐
│Postgres│ │ Redis │  (可选)
│ :5432  │ │ :6379 │
└────────┘ └───────┘
```

- **Nuxt 开发服务器** 把所有 `/api/**` 和 `/health` 请求代理给后端，所以浏览器只跟 `:3000` 打交道。
- 因为这个代理，本地开发没有 CORS 问题。

---

## 数据库

### 连接信息

| 字段 | 值 |
|---|---|
| Host | `localhost` |
| Port | `5432` |
| Database | `ops` |
| User | `ops` |
| Password | `ops_dev` |

### 用 psql 连接

```bash
docker compose exec postgres psql -U ops -d ops
```

用 GUI 客户端（pgAdmin / DBeaver / DataGrip）：

```
jdbc:postgresql://localhost:5432/ops
```

### 迁移

迁移在后端启动时通过 SQLx 自动执行。迁移文件位于：

```
backend/src/migrations/
└── 20240101000001_initial.sql
```

完全重置数据库：

```bash
# 停掉所有服务，销毁 volume，重新拉起
docker compose down -v
docker compose up -d postgres
# 等 Postgres 起来，再重启后端 —— 迁移会重新跑
```

### 数据库表结构概览

| 表 | 说明 |
|---|---|
| `users` | 用户账号（username, password_hash, role, tenant_id） |
| `tenants` | 多租户隔离（name, slug, settings） |
| `claude_sessions` | 持久化的 Claude 会话 |
| `skills` | 自定义 Claude Code skills |
| `mcp_servers` | Model Context Protocol 服务配置 |
| `cloud_accounts` | 云账号凭证（secret_arn） |
| `providers` | LLM provider 配置 |
| `audit_logs` | 安全审计日志 |

---

## 用户管理

### 默认用户

后端首次启动会种入：

| 字段 | 值 |
|---|---|
| 用户名 | `admin` |
| 密码 | `admin123` |
| 角色 | `super_admin` |
| 邮箱 | `admin@ops.local` |

> **警告**：任何非本地部署前必须改掉这个密码。

### 新建用户

```bash
# 交互式：提示输入密码和管理员凭证
./scripts/create-user.sh super_admin myuser
./scripts/create-user.sh tenant_admin myuser
```

或者直接用 API：

```bash
# 1. 拿到 JWT token
TOKEN=$(curl -s -X POST http://localhost:8003/api/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"admin123"}' | jq -r '.token')

# 2. 创建用户
curl -s -X POST http://localhost:8003/api/users \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{
    "username": "testuser",
    "password": "test1234",
    "role": "tenant_admin",
    "tenant_id": "<tenant-uuid>"
  }' | jq .
```

---

## API 参考（本地）

### 公开 endpoint

| Method | 路径 | 说明 |
|---|---|---|
| `GET` | `/health` | 健康检查 |
| `POST` | `/api/auth/login` | 登录，返回 JWT |

### 受保护 endpoint（需 Bearer Token）

| Method | 路径 | 说明 |
|---|---|---|
| `POST` | `/api/auth/logout` | 登出 |
| `GET` | `/api/auth/me` | 当前用户信息 |
| `PUT` | `/api/auth/change-password` | 修改密码 |
| `GET` | `/api/tenants` | 列出租户 |
| `POST` | `/api/tenants` | 创建租户 |
| `GET` | `/api/tenants/{id}` | 获取租户 |
| `PUT` | `/api/tenants/{id}` | 更新租户 |
| `DELETE` | `/api/tenants/{id}` | 删除租户 |
| `GET` | `/api/users` | 列出用户 |
| `POST` | `/api/users` | 创建用户 |
| `PUT` | `/api/users/{id}` | 更新用户 |
| `DELETE` | `/api/users/{id}` | 删除用户 |

---

## 常见任务

### 后端 watch 模式

Rust 本身没有 watch 模式，用 `cargo-watch`：

```bash
# 一次性安装
cargo install cargo-watch

# 文件改动自动重建
cd backend
cargo watch -x run
```

### 全量重置

```bash
# 停掉所有服务
docker compose down -v    # -v 会删掉数据库 volume

# 清掉 Rust 构建缓存（如有需要）
rm -rf backend/target

# 清掉 Node modules（如有需要）
rm -rf frontend/node_modules frontend/.nuxt

# 从零开始
./scripts/local-dev.sh
```

### 修改后端端口

编辑 `.env`：

```bash
BACKEND_PORT=8080
```

如果改了默认的 `3080`，`frontend/nuxt.config.ts` 里的代理目标也要同步改：

```typescript
routeRules: {
  '/api/**': {
    proxy: { to: 'http://localhost:8080/api/**' },
  },
},
```

### 查看日志

```bash
# PostgreSQL 日志
docker compose logs -f postgres

# 后端日志在终端 stdout（由 RUST_LOG 控制）
# 调高日志级别：
RUST_LOG=debug cargo run

# 前端日志在终端 stdout
```

---

## 排障

### 端口被占用

```bash
# 查占用端口的进程
lsof -i :3080
lsof -i :3000
lsof -i :5432

# 杀掉进程
kill -9 <PID>
```

### 数据库连接被拒

```bash
# 看 PostgreSQL 是不是在跑
docker compose ps
docker compose logs postgres

# 重启 PostgreSQL
docker compose restart postgres

# 验证连接
docker compose exec postgres pg_isready -U ops
```

### 后端编译错误

```bash
# 更新 Rust 工具链
rustup update

# 清理重建
cd backend
cargo clean
cargo build
```

### 前端 `npm install` 失败

```bash
# 清 npm 缓存
npm cache clean --force

# 删掉 node_modules 重装
cd frontend
rm -rf node_modules .nuxt
npm install
```

### SQLx 迁移错误

如果迁移失败（比如 schema 已经部分应用过）：

```bash
# 核弹：重置数据库
docker compose down -v
docker compose up -d postgres
# 等几秒，重启后端
```

### 脚本权限被拒

```bash
chmod +x scripts/*.sh
```

---

## IDE 配置

### VS Code（推荐插件）

- **rust-analyzer** — Rust 语言支持
- **Vue - Official** — Vue 3 + TypeScript
- **Tailwind CSS IntelliSense** — Tailwind 自动补全
- **SQLx** — Rust 宏里的 SQL 语法高亮
- **i18n Ally** — i18n key 管理

### 建议配置

```jsonc
// .vscode/settings.json
{
  "rust-analyzer.cargo.features": "all",
  "editor.formatOnSave": true,
  "[rust]": {
    "editor.defaultFormatter": "rust-lang.rust-analyzer"
  },
  "[vue]": {
    "editor.defaultFormatter": "Vue.volar"
  }
}
```

---

## 环境对比

| 维度 | 本地 | Dev / Prod（K8s） |
|---|---|---|
| 配置来源 | `.env` 文件 | K8s ConfigMap + Secrets |
| 数据库 | Docker Compose PostgreSQL | Aurora PostgreSQL |
| 密钥 | `.env` 明文 | AWS Secrets Manager → ESO → K8s Secret |
| 前端 | Nuxt 开发服务器（HMR） | EKS 上的 Nuxt SSR（已构建） |
| 后端 | `cargo run`（debug） | EKS 上的 release 二进制 |
| Redis | Docker Compose（可选） | Helm Bitnami Redis |
| 负载均衡 | 无 | AWS ALB（粘性会话） |
| SSL | 无 | ACM 证书 |
