#!/bin/bash
#
# Generate and apply Karpenter EC2NodeClass and NodePool configurations
#
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
IAC_DIR="${1:-$SCRIPT_DIR/../../../iac}"

RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m'

log() { echo -e "${GREEN}[karpenter]${NC} $*"; }
err() { echo -e "${RED}[karpenter]${NC} $*" >&2; }

# Read terraform values
cd "$IAC_DIR"
CLUSTER_NAME=$(terraform output -raw cluster_name 2>/dev/null || echo "")
NODE_IAM_ROLE_NAME=$(terraform output -raw karpenter_node_iam_role_name 2>/dev/null || echo "")
WORKSPACE=$(terraform workspace show 2>/dev/null || echo "default")

if [[ -z "$CLUSTER_NAME" || -z "$NODE_IAM_ROLE_NAME" ]]; then
    err "Cannot read terraform outputs (cluster_name, karpenter_node_iam_role_name)"
    exit 1
fi

# Environment-specific settings
if [[ "$WORKSPACE" == "prod" ]]; then
    INSTANCE_CATEGORIES="m"
    INSTANCE_MIN_GEN="6"
    VOLUME_SIZE="100Gi"
    LIMITS_CPU="1000"
    LIMITS_MEMORY="1000Gi"
else
    INSTANCE_CATEGORIES="t,m"
    INSTANCE_MIN_GEN="2"
    VOLUME_SIZE="30Gi"
    LIMITS_CPU="100"
    LIMITS_MEMORY="100Gi"
fi

log "Cluster: $CLUSTER_NAME"
log "Workspace: $WORKSPACE"
log "Node IAM Role: $NODE_IAM_ROLE_NAME"

# Apply EC2NodeClass
log "Applying EC2NodeClass..."
sed -e "s|\${node_iam_role_name}|${NODE_IAM_ROLE_NAME}|g" \
    -e "s|\${subnetSelectorTermsValue}|${CLUSTER_NAME}|g" \
    -e "s|volumeSize: .*|volumeSize: ${VOLUME_SIZE}|g" \
    "$SCRIPT_DIR/common-ec2nodeclass.yaml" | kubectl apply -f -

# Apply NodePool
log "Applying NodePool..."
sed -e "s|\${instance_categories}|${INSTANCE_CATEGORIES}|g" \
    -e "s|\${instance_min_generation}|${INSTANCE_MIN_GEN}|g" \
    -e "s|\${limits_cpu}|${LIMITS_CPU}|g" \
    -e "s|\${limits_memory}|${LIMITS_MEMORY}|g" \
    "$SCRIPT_DIR/common-nodepool.yaml" | kubectl apply -f -

log "Karpenter configuration applied!"
