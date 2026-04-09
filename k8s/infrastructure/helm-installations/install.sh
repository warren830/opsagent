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
helm repo add grafana https://grafana.github.io/helm-charts 2>/dev/null || true
helm repo add prometheus-community https://prometheus-community.github.io/helm-charts 2>/dev/null || true
helm repo add argo https://argoproj.github.io/argo-helm 2>/dev/null || true
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

# Monitoring namespace (needed by both self-hosted observability and Alloy)
step "Creating monitoring namespace"
kubectl create namespace monitoring --dry-run=client -o yaml | kubectl apply -f -

# Apply observability ExternalSecret (Grafana Cloud credentials for Alloy)
INFRA_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
if [[ -f "$INFRA_DIR/observability-external-secret.yaml" ]]; then
    step "Applying observability ExternalSecret (monitoring namespace)"
    kubectl apply -f "$INFRA_DIR/observability-external-secret.yaml"
    log "Observability ExternalSecret applied"
fi

# Self-hosted observability backends (Mimir + Loki + Tempo)
# Skip with SKIP_OBSERVABILITY=true when using Grafana Cloud
if [[ "$SKIP_OBSERVABILITY" != "true" ]]; then
    step "Installing Mimir (metrics)"
    helm upgrade --install mimir grafana/mimir-distributed \
        -n monitoring \
        -f "$SCRIPT_DIR/mimir-values.yaml" \
        --timeout 600s \
        --wait
    log "Mimir installed"

    step "Installing Loki (logs)"
    helm upgrade --install loki grafana/loki \
        -n monitoring \
        -f "$SCRIPT_DIR/loki-values.yaml" \
        --timeout 600s \
        --wait
    log "Loki installed"

    step "Installing Tempo (traces)"
    helm upgrade --install tempo grafana/tempo \
        -n monitoring \
        -f "$SCRIPT_DIR/tempo-values.yaml" \
        --timeout 600s \
        --wait
    log "Tempo installed"
else
    warn "Skipping self-hosted observability backends (SKIP_OBSERVABILITY=true)"
fi

# Alloy collector + kube-state-metrics — always installed (works with both self-hosted and Grafana Cloud)
step "Installing kube-state-metrics"
helm upgrade --install kube-state-metrics prometheus-community/kube-state-metrics \
    -n monitoring \
    --set nodeSelector."karpenter\.sh/nodepool"=common-nodepool \
    --timeout 300s \
    --wait
log "kube-state-metrics installed"

step "Installing Alloy (collector)"
helm upgrade --install alloy grafana/alloy \
    -n monitoring \
    -f "$SCRIPT_DIR/alloy-values.yaml" \
    --timeout 600s \
    --wait
log "Alloy installed"

# ArgoCD
if [[ "$SKIP_ARGOCD" != "true" ]]; then
    step "Installing ArgoCD"
    kubectl create namespace argocd --dry-run=client -o yaml | kubectl apply -f -
    helm upgrade --install argocd argo/argo-cd \
        -n argocd \
        -f "$SCRIPT_DIR/argocd-values.yaml" \
        --timeout 600s \
        --wait
    log "ArgoCD installed"

    # Print initial admin password
    ARGOCD_PASS=$(kubectl -n argocd get secret argocd-initial-admin-secret -o jsonpath="{.data.password}" 2>/dev/null | base64 -d 2>/dev/null || echo "")
    if [[ -n "$ARGOCD_PASS" ]]; then
        log "ArgoCD admin password: $ARGOCD_PASS"
        log "Access: kubectl port-forward svc/argocd-server -n argocd 8080:443"
    fi
else
    warn "Skipping ArgoCD (SKIP_ARGOCD=true)"
fi

# Argo Rollouts
if [[ "$SKIP_ARGO_ROLLOUTS" != "true" ]]; then
    step "Installing Argo Rollouts"
    kubectl create namespace argo-rollouts --dry-run=client -o yaml | kubectl apply -f -
    helm upgrade --install argo-rollouts argo/argo-rollouts \
        -n argo-rollouts \
        -f "$SCRIPT_DIR/argo-rollouts-values.yaml" \
        --timeout 600s \
        --wait
    log "Argo Rollouts installed"
else
    warn "Skipping Argo Rollouts (SKIP_ARGO_ROLLOUTS=true)"
fi

echo ""
log "All Helm installations complete!"
