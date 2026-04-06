#!/bin/bash
#
# OpenOps - Destroy all infrastructure
# Reverse order: Application → Helm → Terraform
#
set -e

export AWS_PAGER=""

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
IAC_DIR="$SCRIPT_DIR/iac"
K8S_DIR="$SCRIPT_DIR/k8s"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log()  { echo -e "${GREEN}[destroy]${NC} $*"; }
warn() { echo -e "${YELLOW}[destroy]${NC} $*"; }
err()  { echo -e "${RED}[destroy]${NC} $*" >&2; }

print_header() {
    echo ""
    echo -e "${RED}═══════════════════════════════════════════${NC}"
    echo -e "${RED}  $1${NC}"
    echo -e "${RED}═══════════════════════════════════════════${NC}"
    echo ""
}

print_header "DESTROY OpenOps Infrastructure"
echo -e "${RED}WARNING: This will permanently destroy ALL infrastructure.${NC}"
echo ""
read -p "Type 'destroy' to confirm: " confirm
if [[ "$confirm" != "destroy" ]]; then
    log "Cancelled."
    exit 0
fi

# Step 1: Delete K8s application
log "Step 1: Deleting K8s application..."
if kubectl get namespace openops &>/dev/null; then
    "$K8S_DIR/deploy.sh" delete <<< "yes
yes" 2>/dev/null || warn "K8s app deletion had errors (continuing)"
fi

# Step 2: Delete Helm releases
log "Step 2: Deleting Helm releases..."
for release in redis external-secrets metrics-server karpenter aws-load-balancer-controller; do
    local ns="kube-system"
    [[ "$release" == "redis" ]] && ns="openops"
    [[ "$release" == "external-secrets" ]] && ns="external-secrets"
    helm uninstall "$release" -n "$ns" 2>/dev/null || true
done
log "Helm releases deleted."

# Step 3: Terraform destroy
log "Step 3: Terraform destroy..."
cd "$IAC_DIR"

if [[ -d "$IAC_DIR/.terraform" ]]; then
    terraform destroy -auto-approve
    log "Terraform infrastructure destroyed."
else
    warn "Terraform not initialized, skipping."
fi

echo ""
log "All infrastructure destroyed."
