#!/bin/bash
#
# OpenOps - Unified Deployment Script
# One-click deployment of the complete application stack
#
# Deployment flow:
#   1. Deploy AWS infrastructure (Terraform)
#   2. Deploy Kubernetes infrastructure (Helm: ALB Controller, Karpenter, Metrics Server, ESO)
#   3. Build Docker images and push to ECR
#   4. Deploy application to EKS (Frontend & Backend)
#
# Usage:
#   ./deploy-all.sh                # Interactive full deployment
#   ./deploy-all.sh --step 1       # Run step 1 only (Terraform)
#   ./deploy-all.sh --step 2       # Run step 2 only (Helm)
#   ./deploy-all.sh --step 3       # Run step 3 only (Docker build)
#   ./deploy-all.sh --step 4       # Run step 4 only (App deployment)
#   ./deploy-all.sh --help         # Show help

set -e

export AWS_PAGER=""

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

# Directory definitions
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
IAC_DIR="$SCRIPT_DIR/iac"
K8S_DIR="$SCRIPT_DIR/k8s"
BACKEND_DIR="$SCRIPT_DIR/backend"
FRONTEND_DIR="$SCRIPT_DIR/frontend"

# Auto-select AWS profile if not set
if [[ -z "$AWS_PROFILE" ]]; then
    if ! aws sts get-caller-identity &>/dev/null; then
        AVAILABLE_PROFILE=$(aws configure list-profiles 2>/dev/null | head -1)
        if [[ -n "$AVAILABLE_PROFILE" ]]; then
            export AWS_PROFILE="$AVAILABLE_PROFILE"
            echo "Auto-selected AWS profile: $AWS_PROFILE"
        fi
    fi
fi

# Default configuration
SKIP_CONFIRMATION=false
SPECIFIC_STEP=""
TFVARS_FILE="$IAC_DIR/terraform.tfvars"

# Read terraform.tfvars value
_read_tfvar() {
    local key="$1"
    local raw
    raw=$(awk -v k="$key" '$1 == k && $2 == "=" { $1=""; $2=""; sub(/^[[:space:]]+/, ""); print; exit }' "$TFVARS_FILE" 2>/dev/null)
    raw="${raw#\"}"
    raw="${raw%\"}"
    echo "$raw"
}

_write_tfvar() {
    local key="$1" value="$2" type="${3:-string}"
    [[ ! -f "$TFVARS_FILE" ]] && touch "$TFVARS_FILE"
    local new_value
    if [[ "$type" == "bare" ]]; then new_value="$value"; else new_value="\"${value}\""; fi
    if grep -qE "^${key}\s*=" "$TFVARS_FILE" 2>/dev/null; then
        local tmpfile; tmpfile=$(mktemp)
        awk -v k="$key" -v v="$new_value" '$1 == k && $2 == "=" { print k " = " v; next } { print }' "$TFVARS_FILE" > "$tmpfile" && mv "$tmpfile" "$TFVARS_FILE"
    else
        echo "${key} = ${new_value}" >> "$TFVARS_FILE"
    fi
}

_tf_output() {
    local val
    val=$(cd "$IAC_DIR" && terraform output -raw "$1" 2>/dev/null) || return 1
    [[ "$val" != *"Warning:"* && "$val" != *"Error:"* ]] && echo "$val"
}

# Logging
log()  { echo -e "${GREEN}[openops]${NC} $*"; }
warn() { echo -e "${YELLOW}[openops]${NC} $*"; }
err()  { echo -e "${RED}[openops]${NC} $*" >&2; }

print_header() {
    echo ""
    echo -e "${BLUE}═══════════════════════════════════════════${NC}"
    echo -e "${BLUE}  $1${NC}"
    echo -e "${BLUE}═══════════════════════════════════════════${NC}"
    echo ""
}

print_help() {
    echo "Usage: $0 [--step N] [--yes]"
    echo ""
    echo "Steps:"
    echo "  0  Configure terraform.tfvars"
    echo "  1  Terraform Apply (VPC, EKS, RDS, Secrets)"
    echo "  2  Helm Install (ALB Controller, Karpenter, ESO, Redis)"
    echo "  3  Build & Push Docker images to ECR"
    echo "  4  Deploy application to EKS"
    echo ""
    echo "Options:"
    echo "  --step N    Run specific step only"
    echo "  --yes       Skip confirmation prompts"
    echo "  --help      Show this help"
}

# Parse args
while [[ $# -gt 0 ]]; do
    case "$1" in
        --step) SPECIFIC_STEP="$2"; shift 2 ;;
        --yes)  SKIP_CONFIRMATION=true; shift ;;
        --help) print_help; exit 0 ;;
        *) err "Unknown option: $1"; print_help; exit 1 ;;
    esac
done

run_step() { [[ -z "$SPECIFIC_STEP" ]] || [[ "$SPECIFIC_STEP" == "$1" ]]; }

# ── Step 0: Configure ───────────────────────────────────────
if run_step 0; then
    print_header "Step 0: Configure terraform.tfvars"

    detected_account=$(aws sts get-caller-identity --query Account --output text 2>/dev/null || echo "")
    existing_region=$(_read_tfvar "region" 2>/dev/null || echo "")
    detected_region="${existing_region:-${AWS_REGION:-$(aws configure get region 2>/dev/null || echo "")}}"

    if [[ -n "$detected_account" ]]; then
        _write_tfvar "account" "$detected_account"
        log "account = $detected_account"
    fi
    if [[ -n "$detected_region" ]]; then
        _write_tfvar "region" "$detected_region"
        log "region = $detected_region"
        export AWS_REGION="$detected_region"
    fi

    # Auto-detect feature flags from terraform state
    if [[ -d "$IAC_DIR/.terraform" ]]; then
        cd "$IAC_DIR"
        if terraform state list 2>/dev/null | grep -q "module.waf"; then
            _write_tfvar "enable_waf" "true" "bare"
            log "enable_waf = true (from state)"
        fi
        cd "$SCRIPT_DIR"
    fi

    log "Configuration complete!"
fi

# ── Step 1: Terraform Apply ─────────────────────────────────
if run_step 1; then
    print_header "Step 1: Terraform Apply (VPC, EKS, RDS, Secrets)"

    cd "$IAC_DIR"

    # Initialize terraform if needed
    if [[ ! -d "$IAC_DIR/.terraform" ]]; then
        log "Initializing Terraform..."
        terraform init -upgrade
    fi

    terraform plan -out=tfplan

    if [[ "$SKIP_CONFIRMATION" != "true" ]]; then
        read -p "Apply this plan? (yes/no): " confirm
        if [[ "$confirm" != "yes" ]]; then
            log "Cancelled."
            rm -f tfplan
            exit 0
        fi
    fi

    terraform apply tfplan
    rm -f tfplan

    log "Infrastructure provisioned!"
    cd "$SCRIPT_DIR"
fi

# ── Step 2: Helm Install ────────────────────────────────────
if run_step 2; then
    print_header "Step 2: Helm Install (K8s Infrastructure)"

    cd "$IAC_DIR"
    CLUSTER_NAME=$(_tf_output cluster_name)
    AWS_REGION=$(_tf_output region)

    # Update kubeconfig
    log "Updating kubeconfig..."
    aws eks update-kubeconfig --region "$AWS_REGION" --name "$CLUSTER_NAME"

    # Generate helm values
    log "Generating Helm values from Terraform outputs..."
    "$K8S_DIR/infrastructure/helm-installations/generate-values.sh"

    # Install all helm charts
    log "Installing Helm charts..."
    "$K8S_DIR/infrastructure/helm-installations/install.sh"

    log "K8s infrastructure ready!"
    cd "$SCRIPT_DIR"
fi

# ── Step 3: Build & Push Docker Images ──────────────────────
if run_step 3; then
    print_header "Step 3: Build & Push Docker Images to ECR"

    cd "$SCRIPT_DIR"
    "$SCRIPT_DIR/build-and-push.sh"

    log "Docker images pushed to ECR!"
fi

# ── Step 4: Deploy Application ──────────────────────────────
if run_step 4; then
    print_header "Step 4: Deploy Application to EKS"

    cd "$K8S_DIR"

    # Initialize K8s configs
    log "Initializing K8s configuration..."
    "$K8S_DIR/deploy.sh" init

    # Deploy application
    log "Deploying application..."
    "$K8S_DIR/deploy.sh" deploy

    log "Application deployed!"
    cd "$SCRIPT_DIR"
fi

echo ""
log "Deployment complete!"
