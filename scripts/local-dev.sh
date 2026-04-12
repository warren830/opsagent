#!/usr/bin/env bash
#
# Ops - Local Development Environment
# Starts PostgreSQL via Docker and both backend/frontend dev servers.
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'
log()  { echo -e "${GREEN}[local-dev]${NC} $*"; }
warn() { echo -e "${YELLOW}[local-dev]${NC} $*"; }

# Check prerequisites
command -v docker >/dev/null || { echo "docker required"; exit 1; }
command -v cargo >/dev/null || { echo "cargo (Rust) required"; exit 1; }
command -v node >/dev/null || { echo "node required"; exit 1; }

cd "$PROJECT_ROOT"

# Copy .env if not exists
if [[ ! -f .env ]]; then
  cp .env.example .env
  warn "Created .env from .env.example — review and update values"
fi

# Start database
log "Starting PostgreSQL..."
docker compose up -d postgres

# Wait for database
log "Waiting for database..."
for i in {1..30}; do
  if docker compose exec -T postgres pg_isready -U ops >/dev/null 2>&1; then
    break
  fi
  sleep 1
done

# Seed development data (idempotent — only fills empty tables)
log "Seeding development data..."
docker compose exec -T postgres psql -U ops -d ops -f /dev/stdin < "$SCRIPT_DIR/seed-dev-data.sql" 2>/dev/null || warn "Seed skipped (tables may not exist yet, will retry after backend starts)"

# Start backend (prefixed output so logs don't get lost)
log "Starting Rust backend on :3080..."
(cd backend && cargo run 2>&1 | sed -u "s/^/[backend] /") &
BACKEND_PID=$!

# Wait for backend to be ready (runs migrations on startup)
log "Waiting for backend..."
for i in {1..30}; do
  if curl -s http://localhost:3080/health >/dev/null 2>&1; then
    break
  fi
  sleep 1
done

# Retry seed after migrations have run
docker compose exec -T postgres psql -U ops -d ops -f /dev/stdin < "$SCRIPT_DIR/seed-dev-data.sql" >/dev/null 2>&1 && log "Development data seeded" || true

# Start frontend (prefixed output)
log "Starting Nuxt frontend on :3000..."
(cd frontend && npm run dev 2>&1 | sed -u "s/^/[frontend] /") &
FRONTEND_PID=$!

TUNNEL_PID=""

# Cloudflare tunnel for Grafana webhook — disabled by default, enable with ENABLE_TUNNEL=true
if [ "${ENABLE_TUNNEL:-false}" = "true" ] && command -v cloudflared >/dev/null; then
  log "Starting cloudflare tunnel for Grafana webhook callbacks..."
  cloudflared tunnel --url http://localhost:3080 2>&1 | sed -u "s/^/[tunnel] /" &
  TUNNEL_PID=$!
  sleep 3
  log "💡 Copy the tunnel URL from [tunnel] output → Grafana Alerting → Contact Points → Webhook"
fi

log "🚀 Ops is running!"
log "   Frontend: http://localhost:3000"
log "   Backend:  http://localhost:3080"
log "   Login:    admin / admin123"
log ""
log "Press Ctrl+C to stop all services"

# Cleanup on exit
cleanup() {
  log "Shutting down..."
  kill $BACKEND_PID $FRONTEND_PID $TUNNEL_PID 2>/dev/null || true
  docker compose stop
}
trap cleanup EXIT

wait
