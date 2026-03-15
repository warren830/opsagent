#!/bin/bash
# kubectl-all.sh - Execute kubectl command across all configured clusters
# Usage: ./scripts/kubectl-all.sh get pods -A
# Output: Results prefixed with cluster context name

set -euo pipefail

if [ $# -eq 0 ]; then
  echo "Usage: $0 <kubectl-args...>"
  echo "Example: $0 get pods -A"
  exit 1
fi

KUBECONFIG="${KUBECONFIG:-$HOME/.kube/config}"
contexts=$(kubectl config get-contexts -o name 2>/dev/null || true)

if [ -z "$contexts" ]; then
  echo "No kubectl contexts found."
  exit 0
fi

for ctx in $contexts; do
  echo "=== Cluster: $ctx ==="
  kubectl --context "$ctx" "$@" 2>/dev/null || echo "(failed or no results)"
  echo ""
done
