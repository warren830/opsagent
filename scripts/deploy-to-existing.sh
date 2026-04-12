#!/bin/bash
#
# Ops - Deploy to an existing EKS cluster
# Skips Terraform (Step 1) and Helm (Step 2), only builds and deploys the application
#
# Use this when:
# - Infrastructure already exists
# - You only want to update the application code
# - CI/CD pipeline for application-only deployments
#
# Usage:
#   ./deploy-to-existing.sh              # Build + Deploy
#   ./deploy-to-existing.sh --skip-build # Deploy only (skip Docker build)
#
set -e

export AWS_PAGER=""

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
K8S_DIR="$PROJECT_ROOT/k8s"

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

log()  { echo -e "${GREEN}[deploy]${NC} $*"; }
warn() { echo -e "${YELLOW}[deploy]${NC} $*"; }

SKIP_BUILD=false
[[ "$1" == "--skip-build" ]] && SKIP_BUILD=true

# Step 1: Build and push (unless skipped)
if [[ "$SKIP_BUILD" != "true" ]]; then
    log "Building and pushing Docker images..."
    "$SCRIPT_DIR/build-and-push.sh"
fi

# Step 2: Init and deploy K8s application
log "Initializing K8s configuration..."
"$K8S_DIR/deploy.sh" init

log "Deploying application..."
"$K8S_DIR/deploy.sh" deploy

log "Done! Application deployed to existing cluster."
