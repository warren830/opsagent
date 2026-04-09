#!/bin/bash
#
# OpenOps - Kubernetes Application Deployment Script
# Manages deployment, configuration, and lifecycle of the application on EKS
#
# Usage:
#   ./deploy.sh init      # Initialize configuration (interactive)
#   ./deploy.sh deploy    # Deploy application to Kubernetes
#   ./deploy.sh update    # Update configuration and restart pods
#   ./deploy.sh status    # Show deployment status
#   ./deploy.sh logs      # View application logs
#   ./deploy.sh delete    # Delete application deployment
#   ./deploy.sh help      # Show help
#
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APP_DIR="$SCRIPT_DIR/application"
IAC_DIR="$SCRIPT_DIR/../iac"
NAMESPACE="openops"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log()  { echo -e "${GREEN}[deploy]${NC} $*"; }
warn() { echo -e "${YELLOW}[deploy]${NC} $*"; }
err()  { echo -e "${RED}[deploy]${NC} $*" >&2; }
step() { echo -e "\n${BLUE}── $1${NC}"; }

# ── Helper: Read terraform output ────────────────────────────
_tf_output() {
    local val
    val=$(cd "$IAC_DIR" && terraform output -raw "$1" 2>/dev/null) || return 1
    [[ "$val" != *"Warning:"* && "$val" != *"Error:"* ]] && echo "$val"
}

# ── Helper: Read tfvar ───────────────────────────────────────
_read_tfvar() {
    local key="$1"
    local raw
    raw=$(awk -v k="$key" '$1 == k && $2 == "=" { $1=""; $2=""; sub(/^[[:space:]]+/, ""); print; exit }' "$IAC_DIR/terraform.tfvars" 2>/dev/null)
    raw="${raw#\"}"
    raw="${raw%\"}"
    echo "$raw"
}

# ── INIT: Generate configs from templates ────────────────────
cmd_init() {
    step "Initializing OpenOps K8s configuration"

    # Check dependencies
    for cmd in kubectl envsubst; do
        command -v "$cmd" &>/dev/null || { err "$cmd required but not found"; exit 1; }
    done

    # Read values from terraform
    export AWS_REGION="${AWS_REGION:-$(_tf_output region 2>/dev/null || echo "us-east-1")}"
    export AWS_ACCOUNT_ID="${AWS_ACCOUNT_ID:-$(_read_tfvar account 2>/dev/null || echo "")}"
    export FRONTEND_DOMAIN="${FRONTEND_DOMAIN:-$(_read_tfvar frontend_domain 2>/dev/null || echo "")}"
    export API_DOMAIN="${API_DOMAIN:-$(_read_tfvar api_domain 2>/dev/null || echo "")}"
    export SECRETS_MANAGER_NAME="${SECRETS_MANAGER_NAME:-$(_tf_output backend_secrets_manager_name 2>/dev/null || echo "")}"

    # Detect environment from terraform workspace
    local workspace
    workspace=$(cd "$IAC_DIR" && terraform workspace show 2>/dev/null || echo "default")
    export DEPLOY_ENV="${workspace}"

    if [[ -z "$AWS_ACCOUNT_ID" ]]; then
        AWS_ACCOUNT_ID=$(aws sts get-caller-identity --query Account --output text 2>/dev/null || echo "")
        export AWS_ACCOUNT_ID
    fi

    # ACM certificate ARN (single cert covers both domains via SAN)
    local acm_arn="${FRONTEND_ACM_CERT:-$(cd "$IAC_DIR" && _tf_output acm_cert_arn 2>/dev/null || echo "")}"
    export FRONTEND_ACM_CERT="$acm_arn"
    export API_ACM_CERT="$acm_arn"

    log "AWS Account: $AWS_ACCOUNT_ID"
    log "AWS Region:  $AWS_REGION"
    log "Environment: $DEPLOY_ENV"
    log "Frontend:    $FRONTEND_DOMAIN"
    log "API:         $API_DOMAIN"
    log "Frontend ACM: ${FRONTEND_ACM_CERT:-<not set>}"
    log "API ACM:      ${API_ACM_CERT:-<not set>}"

    # Environment-specific resource settings
    if [[ "$DEPLOY_ENV" == "prod" ]]; then
        export BACKEND_CPU_REQUEST="500m"  BACKEND_CPU_LIMIT="1000m"
        export BACKEND_MEMORY_REQUEST="512Mi"  BACKEND_MEMORY_LIMIT="1024Mi"
        export FRONTEND_CPU_REQUEST="50m"  FRONTEND_CPU_LIMIT="200m"
        export FRONTEND_MEMORY_REQUEST="128Mi"  FRONTEND_MEMORY_LIMIT="256Mi"
        export BACKEND_HPA_MIN="2"  BACKEND_HPA_MAX="8"
        export FRONTEND_HPA_MIN="1"  FRONTEND_HPA_MAX="2"
    else
        export BACKEND_CPU_REQUEST="250m"  BACKEND_CPU_LIMIT="500m"
        export BACKEND_MEMORY_REQUEST="384Mi"  BACKEND_MEMORY_LIMIT="768Mi"
        export FRONTEND_CPU_REQUEST="30m"  FRONTEND_CPU_LIMIT="100m"
        export FRONTEND_MEMORY_REQUEST="64Mi"  FRONTEND_MEMORY_LIMIT="128Mi"
        export BACKEND_HPA_MIN="1"  BACKEND_HPA_MAX="5"
        export FRONTEND_HPA_MIN="1"  FRONTEND_HPA_MAX="2"
    fi

    # Generate configs from templates
    log "Generating K8s manifests from templates..."
    for tmpl in "$APP_DIR"/*.template; do
        local out="${tmpl%.template}"
        envsubst < "$tmpl" > "$out"
        log "  Generated: $(basename "$out")"
    done

    log "Init complete! Run: ./deploy.sh deploy"
}

# ── DEPLOY: Apply all K8s resources ──────────────────────────
cmd_deploy() {
    step "Deploying OpenOps to Kubernetes"

    # 1. Create namespace
    log "Creating namespace..."
    kubectl apply -f "$APP_DIR/namespace.yaml"

    # 2. Create service account for backend (Pod Identity)
    log "Creating service account..."
    kubectl create serviceaccount backend -n $NAMESPACE 2>/dev/null || true

    # 3. Deploy secrets (ESO)
    if [[ -f "$APP_DIR/secret-store.yaml" ]]; then
        log "Deploying ClusterSecretStore..."
        kubectl apply -f "$APP_DIR/secret-store.yaml"
    fi
    if [[ -f "$APP_DIR/external-secret.yaml" ]]; then
        log "Deploying ExternalSecret..."
        kubectl apply -f "$APP_DIR/external-secret.yaml"
    fi

    # 4. Wait for ESO sync
    log "Waiting for secrets to sync (up to 120s)..."
    local waited=0
    while [[ $waited -lt 120 ]]; do
        if kubectl get secret backend-secrets -n $NAMESPACE &>/dev/null; then
            log "Secrets synced!"
            break
        fi
        sleep 5
        waited=$((waited + 5))
    done
    if [[ $waited -ge 120 ]]; then
        warn "Timeout waiting for secrets. Continuing anyway..."
    fi

    # 5. Deploy ConfigMaps
    log "Deploying ConfigMaps..."
    kubectl apply -f "$APP_DIR/backend-configmap.yaml"
    kubectl apply -f "$APP_DIR/frontend-configmap.yaml"

    # 6. Deploy applications
    log "Deploying backend..."
    kubectl apply -f "$APP_DIR/backend-deployment.yaml"
    kubectl apply -f "$APP_DIR/backend-service.yaml"

    log "Deploying frontend..."
    kubectl apply -f "$APP_DIR/frontend-deployment.yaml"
    kubectl apply -f "$APP_DIR/frontend-service.yaml"

    # 7. Deploy Ingress
    if [[ -f "$APP_DIR/ingress-api.yaml" ]]; then
        log "Deploying Ingress..."
        kubectl apply -f "$APP_DIR/ingress-api.yaml"
        kubectl apply -f "$APP_DIR/ingress-frontend.yaml"
    else
        warn "Ingress files not found. Run init first or create manually."
    fi

    # 8. Deploy HPA
    if [[ -f "$APP_DIR/hpa-backend.yaml" ]]; then
        log "Configuring HPA..."
        kubectl apply -f "$APP_DIR/hpa-backend.yaml"
        kubectl apply -f "$APP_DIR/hpa-frontend.yaml"
    fi

    echo ""
    log "Deployment complete!"
    cmd_status
}

# ── STATUS: Show deployment status ───────────────────────────
cmd_status() {
    step "OpenOps Deployment Status"

    echo ""
    log "Pods:"
    kubectl get pods -n $NAMESPACE -o wide 2>/dev/null || warn "No pods found"

    echo ""
    log "Services:"
    kubectl get svc -n $NAMESPACE 2>/dev/null || true

    echo ""
    log "Ingress:"
    kubectl get ingress -n $NAMESPACE 2>/dev/null || true

    echo ""
    log "HPA:"
    kubectl get hpa -n $NAMESPACE 2>/dev/null || true
}

# ── LOGS: View application logs ──────────────────────────────
cmd_logs() {
    local app="${1:-backend}"
    log "Showing logs for $app..."
    kubectl logs -n $NAMESPACE -l app="$app" --tail=100 -f
}

# ── UPDATE: Refresh configs and restart ──────────────────────
cmd_update() {
    step "Updating OpenOps configuration"
    cmd_init
    kubectl apply -f "$APP_DIR/backend-configmap.yaml"
    kubectl apply -f "$APP_DIR/frontend-configmap.yaml"
    kubectl rollout restart deployment/backend -n $NAMESPACE
    kubectl rollout restart deployment/frontend -n $NAMESPACE
    log "Update complete. Pods are restarting..."
}

# ── DELETE: Remove all resources ─────────────────────────────
cmd_delete() {
    step "Deleting OpenOps deployment"
    echo -e "${RED}This will delete all OpenOps resources from the cluster.${NC}"
    read -p "Are you sure? (yes/no): " confirm
    if [[ "$confirm" != "yes" ]]; then
        log "Cancelled."
        return
    fi

    log "Deleting resources..."
    kubectl delete hpa --all -n $NAMESPACE 2>/dev/null || true
    kubectl delete ingress --all -n $NAMESPACE 2>/dev/null || true
    kubectl delete svc --all -n $NAMESPACE 2>/dev/null || true
    kubectl delete deployment --all -n $NAMESPACE 2>/dev/null || true
    kubectl delete configmap backend-config frontend-config -n $NAMESPACE 2>/dev/null || true
    kubectl delete externalsecret backend-secrets -n $NAMESPACE 2>/dev/null || true
    kubectl delete secret backend-secrets -n $NAMESPACE 2>/dev/null || true
    kubectl delete clustersecretstore aws-secrets-manager 2>/dev/null || true
    kubectl delete serviceaccount backend -n $NAMESPACE 2>/dev/null || true

    read -p "Delete namespace '$NAMESPACE'? (yes/no): " del_ns
    if [[ "$del_ns" == "yes" ]]; then
        kubectl delete namespace $NAMESPACE 2>/dev/null || true
    fi

    # Clean generated files
    rm -f "$APP_DIR"/*.yaml.template.bak
    for tmpl in "$APP_DIR"/*.template; do
        rm -f "${tmpl%.template}"
    done

    log "Cleanup complete."
}

# ── HELP ─────────────────────────────────────────────────────
cmd_help() {
    echo "Usage: $0 <command>"
    echo ""
    echo "Commands:"
    echo "  init      Initialize configuration from Terraform outputs"
    echo "  deploy    Deploy application to Kubernetes"
    echo "  update    Update configuration and restart pods"
    echo "  status    Show deployment status"
    echo "  logs      View application logs (default: backend)"
    echo "  delete    Delete application deployment"
    echo "  help      Show this help"
}

# ── Main ─────────────────────────────────────────────────────
case "${1:-help}" in
    init)    cmd_init ;;
    deploy)  cmd_deploy ;;
    update)  cmd_update ;;
    status)  cmd_status ;;
    logs)    cmd_logs "${2:-backend}" ;;
    delete)  cmd_delete ;;
    help)    cmd_help ;;
    *)       err "Unknown command: $1"; cmd_help; exit 1 ;;
esac
