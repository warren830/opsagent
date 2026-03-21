#!/bin/bash
# foreach-account.sh - Execute a command across all AWS Organization accounts
# Automatically discovers accounts from Organizations and assumes a role in each.
set -euo pipefail

ROLE_NAME="${OPS_AGENT_ROLE_NAME:-OpsAgentReadOnly}"
FILTER_ACCOUNTS=""

while [[ "${1:-}" == --* ]]; do
  case "$1" in
    --accounts) FILTER_ACCOUNTS="$2"; shift 2 ;;
    *) break ;;
  esac
done

if [ $# -eq 0 ]; then
  echo "Usage: $0 [--accounts id1,id2,...] <command> [args...]"
  echo ""
  echo "Execute a command in every active AWS Organization account."
  echo "The script assumes the role specified by OPS_AGENT_ROLE_NAME (default: OpsAgentReadOnly)."
  echo ""
  echo "Options:"
  echo "  --accounts id1,id2,...  Only query these account IDs (comma-separated)"
  echo ""
  echo "Examples:"
  echo "  $0 aws eks list-clusters --output json"
  echo "  $0 --accounts 111111111111,222222222222 aws s3 ls"
  echo ""
  echo "Environment variables:"
  echo "  OPS_AGENT_ROLE_NAME  IAM role to assume in each account (default: OpsAgentReadOnly)"
  exit 1
fi

accounts=$(aws organizations list-accounts --query 'Accounts[?Status==`ACTIVE`].[Id,Name]' --output text)

if [ -z "$accounts" ]; then
  echo "No active accounts found in the organization."
  exit 0
fi

while IFS=$'\t' read -r account_id account_name; do
  if [ -n "$FILTER_ACCOUNTS" ]; then
    echo ",$FILTER_ACCOUNTS," | grep -q ",${account_id}," || continue
  fi
  echo "=== ${account_name} (${account_id}) ==="

  creds=$(aws sts assume-role \
    --role-arn "arn:aws:iam::${account_id}:role/${ROLE_NAME}" \
    --role-session-name "ops-agent" \
    --output json 2>/dev/null) || { echo "  [SKIP] Cannot assume role ${ROLE_NAME}"; continue; }

  export AWS_ACCESS_KEY_ID=$(echo "$creds" | jq -r '.Credentials.AccessKeyId')
  export AWS_SECRET_ACCESS_KEY=$(echo "$creds" | jq -r '.Credentials.SecretAccessKey')
  export AWS_SESSION_TOKEN=$(echo "$creds" | jq -r '.Credentials.SessionToken')

  "$@" || echo "  [ERROR] Command failed"

  unset AWS_ACCESS_KEY_ID AWS_SECRET_ACCESS_KEY AWS_SESSION_TOKEN
done <<< "$accounts"
