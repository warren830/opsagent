#!/bin/bash
# scan-expiring-certs.sh - Scan ACM certificates expiring within N days across all accounts
set -euo pipefail

ROLE_NAME="${OPS_AGENT_ROLE_NAME:-OpsAgentReadOnly}"
DAYS="${1:-30}"
REGIONS="${OPS_AGENT_REGIONS:-}"

usage() {
  echo "Usage: $0 [days] [-r region1,region2,...] [-h]"
  echo ""
  echo "Scan for ACM certificates expiring within the specified number of days"
  echo "across all Organization accounts."
  echo ""
  echo "Arguments:"
  echo "  days        Number of days to look ahead (default: 30)"
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

# Parse arguments: first positional arg is days, then flags
POSITIONAL_SET=false
while [[ $# -gt 0 ]]; do
  case $1 in
    -r) REGIONS="$2"; shift 2 ;;
    -h) usage; exit 0 ;;
    -*)  usage; exit 1 ;;
    *)
      if [ "$POSITIONAL_SET" = false ]; then
        DAYS="$1"
        POSITIONAL_SET=true
      fi
      shift
      ;;
  esac
done

if ! [[ "$DAYS" =~ ^[0-9]+$ ]]; then
  echo "Error: days must be a positive integer."
  exit 1
fi

get_regions() {
  if [ -n "$REGIONS" ]; then
    echo "$REGIONS" | tr ',' '\n'
  else
    aws ec2 describe-regions --query 'Regions[].RegionName' --output text | tr '\t' '\n'
  fi
}

# Calculate the cutoff date (now + DAYS) in epoch seconds
if date --version >/dev/null 2>&1; then
  # GNU date
  cutoff_epoch=$(date -d "+${DAYS} days" +%s)
else
  # BSD/macOS date
  cutoff_epoch=$(date -v+${DAYS}d +%s)
fi

accounts=$(aws organizations list-accounts --query 'Accounts[?Status==`ACTIVE`].[Id,Name]' --output text)

if [ -z "$accounts" ]; then
  echo "No active accounts found."
  exit 0
fi

echo "Scanning for ACM certificates expiring within ${DAYS} days..."
echo ""

found=0

while IFS=$'\t' read -r account_id account_name; do
  creds=$(aws sts assume-role \
    --role-arn "arn:aws:iam::${account_id}:role/${ROLE_NAME}" \
    --role-session-name "ops-agent" \
    --output json 2>/dev/null) || { continue; }

  export AWS_ACCESS_KEY_ID=$(echo "$creds" | jq -r '.Credentials.AccessKeyId')
  export AWS_SECRET_ACCESS_KEY=$(echo "$creds" | jq -r '.Credentials.SecretAccessKey')
  export AWS_SESSION_TOKEN=$(echo "$creds" | jq -r '.Credentials.SessionToken')

  region_list=$(get_regions)

  while read -r region; do
    [ -z "$region" ] && continue

    certs=$(aws acm list-certificates --region "$region" \
      --query 'CertificateSummaryList[].CertificateArn' --output text 2>/dev/null) || continue
    [ -z "$certs" ] && continue

    for arn in $certs; do
      cert_info=$(aws acm describe-certificate --certificate-arn "$arn" --region "$region" \
        --query 'Certificate.[DomainName,NotAfter,Status]' --output text 2>/dev/null) || continue

      domain=$(echo "$cert_info" | awk '{print $1}')
      not_after=$(echo "$cert_info" | awk '{print $2}')
      status=$(echo "$cert_info" | awk '{print $3}')

      [ "$status" != "ISSUED" ] && continue
      [ -z "$not_after" ] || [ "$not_after" = "None" ] && continue

      # Convert expiry to epoch
      if date --version >/dev/null 2>&1; then
        cert_epoch=$(date -d "$not_after" +%s 2>/dev/null) || continue
      else
        cert_epoch=$(date -jf "%Y-%m-%dT%H:%M:%S" "$not_after" +%s 2>/dev/null) || \
        cert_epoch=$(date -jf "%Y-%m-%dT%H:%M:%S%z" "$not_after" +%s 2>/dev/null) || continue
      fi

      if [ "$cert_epoch" -le "$cutoff_epoch" ]; then
        now_epoch=$(date +%s)
        days_left=$(( (cert_epoch - now_epoch) / 86400 ))

        if [ "$days_left" -lt 0 ]; then
          label="EXPIRED"
        else
          label="${days_left} days left"
        fi

        printf "%-30s %-14s %-15s %-40s %s (%s)\n" \
          "$account_name" "$account_id" "$region" "$domain" "$not_after" "$label"
        found=$((found + 1))
      fi
    done
  done <<< "$region_list"

  unset AWS_ACCESS_KEY_ID AWS_SECRET_ACCESS_KEY AWS_SESSION_TOKEN
done <<< "$accounts"

echo ""
if [ "$found" -eq 0 ]; then
  echo "No certificates expiring within ${DAYS} days."
else
  echo "Total expiring/expired certificates: ${found}"
fi
