#!/usr/bin/env bash
# kubectl-safe.sh — kubectl wrapper for approval workflow
# Read operations execute directly; write operations submit an approval request.
#
# Usage: ./scripts/kubectl-safe.sh [kubectl args...]
# Example: ./scripts/kubectl-safe.sh --context prod delete pod my-pod

set -euo pipefail

OPSAGENT_URL="${OPSAGENT_URL:-http://localhost:3978}"

# Write-operation subcommands that require approval
WRITE_OPS="apply|create|delete|patch|edit|scale|rollout|drain|cordon|uncordon|taint|label|annotate|replace|set"

# Extract the kubectl subcommand (skip flags like --context, --namespace)
subcmd=""
skip=false
for arg in "$@"; do
  if $skip; then
    skip=false
    continue
  fi
  case "$arg" in
    --context|--namespace|-n|--kubeconfig|-l|--selector|-o|--output|--field-selector)
      skip=true
      continue
      ;;
    --*=*) continue ;;
    --*) continue ;;
    -*)  continue ;;
    *)
      subcmd="$arg"
      break
      ;;
  esac
done

# If no subcommand found or it's a read operation, execute directly
if [ -z "$subcmd" ]; then
  exec kubectl "$@"
fi

if echo "$subcmd" | grep -qwE "$WRITE_OPS"; then
  # Write operation — submit approval request
  full_cmd="kubectl $*"
  echo "⚠️  kubectl write operation detected: $subcmd"
  echo "Submitting approval request..."

  response=$(curl -s -X POST "${OPSAGENT_URL}/admin/api/approvals" \
    -H "Content-Type: application/json" \
    -d "{
      \"command\": $(printf '%s' "$full_cmd" | jq -Rs .),
      \"requestedBy\": \"${OPSAGENT_USER:-agent}\",
      \"requestedByName\": \"${OPSAGENT_USER_NAME:-OpsAgent}\",
      \"platform\": \"${OPSAGENT_PLATFORM:-cli}\",
      \"channelId\": \"${OPSAGENT_CHANNEL:-}\"
    }" 2>/dev/null || echo '{"error":"Failed to connect to OpsAgent API"}')

  if echo "$response" | jq -e '.id' >/dev/null 2>&1; then
    approval_id=$(echo "$response" | jq -r '.id')
    echo "Approval request #${approval_id} created."
    echo "Command: $full_cmd"
    echo "Waiting for admin approval at ${OPSAGENT_URL}/admin"
  else
    echo "ERROR: Failed to submit approval request."
    echo "$response"
    exit 1
  fi
else
  # Read operation — execute directly
  exec kubectl "$@"
fi
