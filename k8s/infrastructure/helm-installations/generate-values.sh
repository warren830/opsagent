#!/bin/bash
#
# Generate Helm values files from Terraform outputs
#
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
IAC_DIR="${SCRIPT_DIR}/../../../iac"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

log()  { echo -e "${GREEN}[generate-values]${NC} $*"; }
warn() { echo -e "${YELLOW}[generate-values]${NC} $*"; }
err()  { echo -e "${RED}[generate-values]${NC} $*" >&2; }

# Read terraform output
_tf_output() {
    local name="$1"
    local val
    val=$(cd "$IAC_DIR" && terraform output -raw "$name" 2>/dev/null) || return 1
    if [[ "$val" == *"Warning:"* || "$val" == *"Error:"* ]]; then
        return 1
    fi
    echo "$val"
}

# Get values from env or terraform
CLUSTER_NAME="${CLUSTER_NAME:-$(_tf_output cluster_name 2>/dev/null || echo "")}"
VPC_ID="${VPC_ID:-$(_tf_output vpc_id 2>/dev/null || echo "")}"
AWS_REGION="${AWS_REGION:-$(_tf_output region 2>/dev/null || echo "")}"
CLUSTER_ENDPOINT="${CLUSTER_ENDPOINT:-$(_tf_output cluster_endpoint 2>/dev/null || echo "")}"
KARPENTER_SERVICE_ACCOUNT="${KARPENTER_SERVICE_ACCOUNT:-$(_tf_output karpenter_service_account 2>/dev/null || echo "karpenter")}"
KARPENTER_QUEUE_NAME="${KARPENTER_QUEUE_NAME:-$(_tf_output karpenter_queue_name 2>/dev/null || echo "")}"

# Validate required values
if [[ -z "$CLUSTER_NAME" || -z "$AWS_REGION" ]]; then
    err "Cannot determine CLUSTER_NAME or AWS_REGION"
    err "Either set them as environment variables or ensure terraform outputs are available"
    exit 1
fi

# Generate ALBC values
log "Generating aws-load-balancer-controller-values.yaml..."
cat > "$SCRIPT_DIR/aws-load-balancer-controller-values.yaml" <<EOF
clusterName: ${CLUSTER_NAME}
region: ${AWS_REGION}
vpcId: ${VPC_ID}
serviceAccount:
  name: aws-load-balancer-controller
EOF
log "Written: aws-load-balancer-controller-values.yaml"

# Generate Karpenter values (if endpoint available)
if [[ -n "$CLUSTER_ENDPOINT" && -n "$KARPENTER_QUEUE_NAME" ]]; then
    log "Generating karpenter-values.yaml..."
    cat > "$SCRIPT_DIR/karpenter-values.yaml" <<EOF
serviceAccount:
  name: ${KARPENTER_SERVICE_ACCOUNT}
settings:
  clusterName: ${CLUSTER_NAME}
  clusterEndpoint: ${CLUSTER_ENDPOINT}
  interruptionQueue: ${KARPENTER_QUEUE_NAME}
EOF
    log "Written: karpenter-values.yaml"
else
    warn "Skipping karpenter-values.yaml (missing CLUSTER_ENDPOINT or KARPENTER_QUEUE_NAME)"
fi

log "Done! Values files generated."
