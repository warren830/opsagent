#!/bin/bash
# list-all-clusters.sh - List all EKS clusters across all accounts and regions
# Shows cluster name, version, and status for each account/region combination.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROLE_NAME="${OPS_AGENT_ROLE_NAME:-OpsAgentReadOnly}"
REGIONS="${OPS_AGENT_REGIONS:-}"

usage() {
  echo "Usage: $0 [-r region1,region2,...] [-h]"
  echo ""
  echo "List all EKS clusters across all Organization accounts and regions."
  echo ""
  echo "Options:"
  echo "  -r REGIONS  Comma-separated list of AWS regions to scan"
  echo "              (default: all enabled regions per account)"
  echo "  -h          Show this help message"
  echo ""
  echo "Environment variables:"
  echo "  OPS_AGENT_ROLE_NAME  IAM role to assume (default: OpsAgentReadOnly)"
  echo "  OPS_AGENT_REGIONS    Comma-separated regions to scan"
}

while getopts "r:h" opt; do
  case $opt in
    r) REGIONS="$OPTARG" ;;
    h) usage; exit 0 ;;
    *) usage; exit 1 ;;
  esac
done

get_regions() {
  if [ -n "$REGIONS" ]; then
    echo "$REGIONS" | tr ',' '\n'
  else
    aws ec2 describe-regions --query 'Regions[].RegionName' --output text | tr '\t' '\n'
  fi
}

accounts=$(aws organizations list-accounts --query 'Accounts[?Status==`ACTIVE`].[Id,Name]' --output text)

if [ -z "$accounts" ]; then
  echo "No active accounts found."
  exit 0
fi

total_clusters=0

while IFS=$'\t' read -r account_id account_name; do
  creds=$(aws sts assume-role \
    --role-arn "arn:aws:iam::${account_id}:role/${ROLE_NAME}" \
    --role-session-name "ops-agent" \
    --output json 2>/dev/null) || { echo "[SKIP] ${account_name} (${account_id}) - Cannot assume role"; continue; }

  export AWS_ACCESS_KEY_ID=$(echo "$creds" | jq -r '.Credentials.AccessKeyId')
  export AWS_SECRET_ACCESS_KEY=$(echo "$creds" | jq -r '.Credentials.SecretAccessKey')
  export AWS_SESSION_TOKEN=$(echo "$creds" | jq -r '.Credentials.SessionToken')

  region_list=$(get_regions)

  while read -r region; do
    [ -z "$region" ] && continue

    clusters=$(aws eks list-clusters --region "$region" --query 'clusters[]' --output text 2>/dev/null) || continue
    [ -z "$clusters" ] && continue

    for cluster in $clusters; do
      info=$(aws eks describe-cluster --name "$cluster" --region "$region" \
        --query 'cluster.[version,status]' --output text 2>/dev/null) || continue

      version=$(echo "$info" | awk '{print $1}')
      status=$(echo "$info" | awk '{print $2}')

      printf "%-30s %-14s %-12s %-10s %s\n" "$account_name" "$account_id" "$region" "v${version}" "$cluster ($status)"
      total_clusters=$((total_clusters + 1))
    done
  done <<< "$region_list"

  unset AWS_ACCESS_KEY_ID AWS_SECRET_ACCESS_KEY AWS_SESSION_TOKEN
done <<< "$accounts"

echo ""
echo "Total clusters found: ${total_clusters}"
