#!/bin/bash
#
# Install EKS cluster add-ons via Helm
# Prerequisite: EKS cluster created, IAM roles and Pod Identity Associations exist
#
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log()  { echo -e "${GREEN}[helm-install]${NC} $*"; }
warn() { echo -e "${YELLOW}[helm-install]${NC} $*"; }
err()  { echo -e "${RED}[helm-install]${NC} $*" >&2; }
step() { echo -e "\n${BLUE}── $1${NC}"; }

# Check dependencies
for cmd in kubectl helm; do
    if ! command -v "$cmd" &>/dev/null; then
        err "$cmd is required but not found"
        exit 1
    fi
done

# Add Helm repos
step "Adding Helm repositories"
helm repo add eks https://aws.github.io/eks-charts 2>/dev/null || true
helm repo add metrics-server https://kubernetes-sigs.github.io/metrics-server/ 2>/dev/null || true
helm repo add external-secrets https://charts.external-secrets.io 2>/dev/null || true
helm repo add bitnami https://charts.bitnami.com/bitnami 2>/dev/null || true
helm repo update

# Create gp3 StorageClass (default)
step "Creating gp3 StorageClass"
kubectl annotate storageclass gp2 storageclass.kubernetes.io/is-default-class=false --overwrite 2>/dev/null || true
cat <<EOF | kubectl apply -f -
apiVersion: storage.k8s.io/v1
kind: StorageClass
metadata:
  name: gp3
  annotations:
    storageclass.kubernetes.io/is-default-class: "true"
provisioner: ebs.csi.aws.com
parameters:
  type: gp3
  encrypted: "true"
reclaimPolicy: Delete
volumeBindingMode: WaitForFirstConsumer
allowVolumeExpansion: true
EOF
log "gp3 StorageClass created"

# AWS Load Balancer Controller
if [[ "$SKIP_ALBC" != "true" ]]; then
    step "Installing AWS Load Balancer Controller"
    helm upgrade --install aws-load-balancer-controller eks/aws-load-balancer-controller \
        -n kube-system \
        -f "$SCRIPT_DIR/aws-load-balancer-controller-values.yaml" \
        --timeout 600s \
        --wait
    log "AWS Load Balancer Controller installed"
else
    warn "Skipping AWS Load Balancer Controller (SKIP_ALBC=true)"
fi

# Karpenter
if [[ "$SKIP_KARPENTER" != "true" && -f "$SCRIPT_DIR/karpenter-values.yaml" ]]; then
    step "Installing Karpenter"
    helm upgrade --install karpenter oci://public.ecr.aws/karpenter/karpenter \
        --version "1.9.0" \
        -n kube-system \
        -f "$SCRIPT_DIR/karpenter-values.yaml" \
        --timeout 600s \
        --wait
    log "Karpenter installed"

    # Apply Karpenter node configuration
    if [[ -x "$SCRIPT_DIR/../karpenter/apply-karpenter-config.sh" ]]; then
        "$SCRIPT_DIR/../karpenter/apply-karpenter-config.sh"
    fi
else
    warn "Skipping Karpenter"
fi

# Metrics Server
step "Installing Metrics Server"
helm upgrade --install metrics-server metrics-server/metrics-server \
    -n kube-system \
    -f "$SCRIPT_DIR/metrics-server-values.yaml" \
    --timeout 600s \
    --wait
log "Metrics Server installed"

# External Secrets Operator
step "Installing External Secrets Operator"
helm upgrade --install external-secrets external-secrets/external-secrets \
    -n external-secrets --create-namespace \
    -f "$SCRIPT_DIR/external-secrets-values.yaml" \
    --timeout 600s \
    --wait
log "External Secrets Operator installed"

# Redis
step "Installing Redis"
helm upgrade --install redis bitnami/redis \
    -n openops --create-namespace \
    -f "$SCRIPT_DIR/redis-values.yaml" \
    --timeout 600s \
    --wait
log "Redis installed"

echo ""
log "All Helm installations complete!"
