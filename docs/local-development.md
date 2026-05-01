# Ops Local Development Guide

## Prerequisites

| Tool | Minimum Version | Check Command | Install |
|------|----------------|---------------|---------|
| Docker | 20+ (with Compose V2) | `docker --version` | [docker.com](https://docs.docker.com/get-docker/) |
| Rust | 1.83+ | `rustc --version` | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| Node.js | 20+ | `node --version` | [nodejs.org](https://nodejs.org/) or `brew install node` |
| npm | 10+ | `npm --version` | Bundled with Node.js |

> **macOS users**: `brew install rustup node docker` can install all of the above.

---

## Quick Start (One Command)

```bash
./scripts/local-dev.sh
```

This script will:
1. Check all prerequisites are installed
2. Copy `.env.example` → `.env` (if not exists)
3. Start PostgreSQL via Docker Compose
4. Wait for database to be ready
5. Start Rust backend on `:3080`
6. Start Nuxt frontend on `:3000`

Once everything is running:

```
Frontend:  http://localhost:3003
Backend:   http://localhost:8003
Health:    http://localhost:8003/health
Login:     admin / admin123
```

Press `Ctrl+C` to stop all services.

---

## Manual Setup (Step by Step)

If you prefer to run each component in its own terminal, or need more control:

### Step 1: Environment Variables

```bash
cp .env.example .env
```

The defaults work out of the box for local development. Key settings:

| Variable | Default | Description |
|----------|---------|-------------|
| `OPS_ENV` | `local` | Environment mode |
| `DATABASE_URL` | `postgresql://ops:ops_dev@localhost:5432/ops` | PostgreSQL connection |
| `JWT_SECRET` | `dev-secret-minimum-32-characters-long-change-in-prod` | JWT signing key |
| `BACKEND_PORT` | `3080` | Rust server port |
| `FRONTEND_PORT` | `3000` | Nuxt dev server port |
| `ALLOWED_ORIGINS` | `http://localhost:3003` | CORS allowed origins |
| `RUST_LOG` | `ops=debug,tower_http=debug` | Log level |

### Step 2: Start Database

```bash
# Start PostgreSQL only
docker compose up -d postgres

# Verify it's running
docker compose ps
docker compose exec postgres pg_isready -U ops
```

If you also need Redis (optional for local):

```bash
docker compose up -d
```

### Step 3: Start Backend (Terminal 1)

```bash
cd backend
cargo run
```

First run will:
- Download and compile all Rust dependencies (~2-5 min)
- Run database migrations automatically
- Seed a default admin user (`admin` / `admin123`)
- Start the server on `http://localhost:8003`

You should see:

```
INFO ops: Starting Ops backend (env=Local)
INFO ops::db: Database pool created (max=20, min=5)
INFO ops::db: Running database migrations...
INFO ops: Default admin user created (username: admin, password: admin123)
INFO ops: Listening on 0.0.0.0:3080
```

### Step 4: Start Frontend (Terminal 2)

```bash
cd frontend
npm install   # First time only
npm run dev
```

The Nuxt dev server starts on `http://localhost:3003` with:
- Hot module replacement (HMR)
- API proxy: `/api/**` → `http://localhost:8003/api/**`
- Vue DevTools enabled

### Step 5: Verify

```bash
# Health check
curl http://localhost:8003/health

# Login (get JWT token)
curl -s -X POST http://localhost:8003/api/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"admin123"}' | jq .

# Or open browser
open http://localhost:3003
```

---

## Service Architecture (Local)

```
Browser (:3000)
    │
    ▼
┌──────────────────┐
│  Nuxt Dev Server  │  :3000  (HMR + API Proxy)
│  (frontend/)      │
└────────┬─────────┘
         │ /api/** proxy
         ▼
┌──────────────────┐
│  Axum Backend     │  :3080  (REST API)
│  (backend/)       │
└────────┬─────────┘
         │
    ┌────┴────┐
    ▼         ▼
┌────────┐ ┌───────┐
│Postgres│ │ Redis │  (optional)
│ :5432  │ │ :6379 │
└────────┘ └───────┘
```

- The **Nuxt dev server** proxies all `/api/**` and `/health` requests to the backend, so the browser only talks to `:3000`.
- No CORS issues in local dev thanks to this proxy.

---

## Database

### Connection Info

| Field | Value |
|-------|-------|
| Host | `localhost` |
| Port | `5432` |
| Database | `ops` |
| User | `ops` |
| Password | `ops_dev` |

### Connect with psql

```bash
docker compose exec postgres psql -U ops -d ops
```

Or with a GUI client (e.g., pgAdmin, DBeaver, DataGrip):

```
jdbc:postgresql://localhost:5432/ops
```

### Migrations

Migrations run automatically on backend startup via SQLx. Migration files are in:

```
backend/src/migrations/
└── 20240101000001_initial.sql
```

To reset the database completely:

```bash
# Stop everything, destroy volume, restart
docker compose down -v
docker compose up -d postgres
# Wait for postgres, then restart backend — migrations will re-run
```

### Database Schema Overview

| Table | Description |
|-------|-------------|
| `users` | User accounts (username, password_hash, role, tenant_id) |
| `tenants` | Multi-tenant isolation (name, slug, settings) |
| `claude_sessions` | Persistent Claude chat sessions |
| `skills` | Custom Claude Code skills |
| `mcp_servers` | Model Context Protocol server configs |
| `cloud_accounts` | Cloud provider credentials (secret_arn) |
| `providers` | LLM provider configurations |
| `audit_logs` | Security audit trail |

---

## User Management

### Default User

On first startup, the backend seeds:

| Field | Value |
|-------|-------|
| Username | `admin` |
| Password | `admin123` |
| Role | `super_admin` |
| Email | `admin@ops.local` |

> **Warning**: Change this password before any non-local deployment.

### Create Additional Users

```bash
# Interactive: prompts for password and admin auth
./scripts/create-user.sh super_admin myuser
./scripts/create-user.sh tenant_admin myuser
```

Or via API directly:

```bash
# 1. Get JWT token
TOKEN=$(curl -s -X POST http://localhost:8003/api/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"admin123"}' | jq -r '.token')

# 2. Create user
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

## API Reference (Local)

### Public Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/health` | Health check |
| `POST` | `/api/auth/login` | Login, returns JWT |

### Protected Endpoints (Bearer Token Required)

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/api/auth/logout` | Logout |
| `GET` | `/api/auth/me` | Current user info |
| `PUT` | `/api/auth/change-password` | Change password |
| `GET` | `/api/tenants` | List tenants |
| `POST` | `/api/tenants` | Create tenant |
| `GET` | `/api/tenants/{id}` | Get tenant |
| `PUT` | `/api/tenants/{id}` | Update tenant |
| `DELETE` | `/api/tenants/{id}` | Delete tenant |
| `GET` | `/api/users` | List users |
| `POST` | `/api/users` | Create user |
| `PUT` | `/api/users/{id}` | Update user |
| `DELETE` | `/api/users/{id}` | Delete user |

---

## Common Tasks

### Watch Mode for Backend

Rust doesn't have built-in watch mode. Use `cargo-watch`:

```bash
# Install once
cargo install cargo-watch

# Auto-rebuild on file changes
cd backend
cargo watch -x run
```

### Reset Everything

```bash
# Stop all services
docker compose down -v    # -v removes database volume

# Remove Rust build cache (if needed)
rm -rf backend/target

# Remove Node modules (if needed)
rm -rf frontend/node_modules frontend/.nuxt

# Start fresh
./scripts/local-dev.sh
```

### Change Backend Port

Edit `.env`:

```bash
BACKEND_PORT=8080
```

Also update the proxy target in `frontend/nuxt.config.ts` if changing from default `3080`:

```typescript
routeRules: {
  '/api/**': {
    proxy: { to: 'http://localhost:8080/api/**' },
  },
},
```

### View Logs

```bash
# PostgreSQL logs
docker compose logs -f postgres

# Backend logs are in terminal stdout (controlled by RUST_LOG)
# Increase verbosity:
RUST_LOG=debug cargo run

# Frontend logs are in terminal stdout
```

---

## Troubleshooting

### Port Already in Use

```bash
# Find what's using the port
lsof -i :3080
lsof -i :3000
lsof -i :5432

# Kill the process
kill -9 <PID>
```

### Database Connection Refused

```bash
# Check if PostgreSQL is running
docker compose ps
docker compose logs postgres

# Restart PostgreSQL
docker compose restart postgres

# Verify connection
docker compose exec postgres pg_isready -U ops
```

### Backend Compile Error

```bash
# Update Rust toolchain
rustup update

# Clean and rebuild
cd backend
cargo clean
cargo build
```

### Frontend `npm install` Fails

```bash
# Clear npm cache
npm cache clean --force

# Remove node_modules and reinstall
cd frontend
rm -rf node_modules .nuxt
npm install
```

### SQLx Migration Error

If migrations fail (e.g., schema already partially applied):

```bash
# Nuclear option: reset the database
docker compose down -v
docker compose up -d postgres
# Wait a few seconds, then restart backend
```

### Permission Denied on Scripts

```bash
chmod +x scripts/*.sh
```

---

## IDE Setup

### VS Code (Recommended Extensions)

- **rust-analyzer** — Rust language support
- **Vue - Official** — Vue 3 + TypeScript
- **Tailwind CSS IntelliSense** — Tailwind autocomplete
- **SQLx** — SQL syntax highlighting in Rust macros
- **i18n Ally** — i18n key management

### Settings Suggestion

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

## Environment Comparison

| Aspect | Local | Dev / Prod (K8s) |
|--------|-------|-------------------|
| Config source | `.env` file | K8s ConfigMap + Secrets |
| Database | Docker Compose PostgreSQL | Aurora PostgreSQL |
| Secrets | Plaintext in `.env` | AWS Secrets Manager → ESO → K8s Secret |
| Frontend | Nuxt dev server (HMR) | Nuxt SSR (built) on EKS |
| Backend | `cargo run` (debug) | Release binary on EKS |
| Redis | Docker Compose (optional) | Helm Bitnami Redis |
| Load Balancer | N/A | AWS ALB (sticky sessions) |
| SSL | N/A | ACM certificate |
