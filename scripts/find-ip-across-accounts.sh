#!/bin/bash
# find-ip-across-accounts.sh - Find which account/resource owns a given IP address
# Searches EIPs, ENIs, and ELBs across all Organization accounts and regions.
set -euo pipefail

ROLE_NAME="${OPS_AGENT_ROLE_NAME:-OpsAgentReadOnly}"
REGIONS="${OPS_AGENT_REGIONS:-}"

usage() {
  echo "Usage: $0 <ip-address> [-r region1,region2,...] [-h]"
  echo ""
  echo "Find the owner of an IP address across all Organization accounts."
  echo "Searches Elastic IPs, ENIs, and Load Balancers."
  echo ""
  echo "Arguments:"
  echo "  ip-address  The IP address to search for"
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

if [ $# -eq 0 ]; then
  usage
  exit 1
fi

TARGET_IP=""
while [[ $# -gt 0 ]]; do
  case $1 in
    -r) REGIONS="$2"; shift 2 ;;
    -h) usage; exit 0 ;;
    -*) usage; exit 1 ;;
    *)
      if [ -z "$TARGET_IP" ]; then
        TARGET_IP="$1"
      fi
      shift
      ;;
  esac
done

if [ -z "$TARGET_IP" ]; then
  echo "Error: IP address is required."
  usage
  exit 1
fi

get_regions() {
  if [ -n "$REGIONS" ]; then
    echo "$REGIONS" | tr ',' '\n'
  else
    aws ec2 describe-regions --query 'Regions[].RegionName' --output text | tr '\t' '\n'
  fi
}

echo "Searching for IP: ${TARGET_IP}"
echo ""

found=0

accounts=$(aws organizations list-accounts --query 'Accounts[?Status==`ACTIVE`].[Id,Name]' --output text)

if [ -z "$accounts" ]; then
  echo "No active accounts found."
  exit 0
fi

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

    # Check Elastic IPs
    eip=$(aws ec2 describe-addresses --region "$region" \
      --filters "Name=public-ip,Values=${TARGET_IP}" \
      --query 'Addresses[0].[AllocationId,InstanceId,NetworkInterfaceId]' \
      --output text 2>/dev/null) || true

    if [ -n "$eip" ] && [ "$eip" != "None	None	None" ] && [ "$eip" != "None" ]; then
      alloc_id=$(echo "$eip" | awk '{print $1}')
      instance_id=$(echo "$eip" | awk '{print $2}')
      eni_id=$(echo "$eip" | awk '{print $3}')
      echo "[EIP] Account: ${account_name} (${account_id}) | Region: ${region}"
      echo "      Allocation: ${alloc_id} | Instance: ${instance_id} | ENI: ${eni_id}"
      found=$((found + 1))
    fi

    # Check ENIs (private and public IPs)
    eni_results=$(aws ec2 describe-network-interfaces --region "$region" \
      --filters "Name=addresses.private-ip-address,Values=${TARGET_IP}" \
      --query 'NetworkInterfaces[].[NetworkInterfaceId,Description,PrivateIpAddress,Association.PublicIp]' \
      --output text 2>/dev/null) || true

    if [ -z "$eni_results" ] || [ "$eni_results" = "None" ]; then
      eni_results=$(aws ec2 describe-network-interfaces --region "$region" \
        --filters "Name=association.public-ip,Values=${TARGET_IP}" \
        --query 'NetworkInterfaces[].[NetworkInterfaceId,Description,PrivateIpAddress,Association.PublicIp]' \
        --output text 2>/dev/null) || true
    fi

    if [ -n "$eni_results" ] && [ "$eni_results" != "None" ]; then
      while IFS=$'\t' read -r eni_id description private_ip public_ip; do
        [ -z "$eni_id" ] && continue
        echo "[ENI] Account: ${account_name} (${account_id}) | Region: ${region}"
        echo "      ENI: ${eni_id} | Private: ${private_ip} | Public: ${public_ip}"
        echo "      Description: ${description}"
        found=$((found + 1))
      done <<< "$eni_results"
    fi

    # Check Classic Load Balancers
    elb_names=$(aws elb describe-load-balancers --region "$region" \
      --query 'LoadBalancerDescriptions[].LoadBalancerName' --output text 2>/dev/null) || true

    if [ -n "$elb_names" ] && [ "$elb_names" != "None" ]; then
      for elb_name in $elb_names; do
        dns=$(aws elb describe-load-balancers --region "$region" \
          --load-balancer-names "$elb_name" \
          --query 'LoadBalancerDescriptions[0].DNSName' --output text 2>/dev/null) || continue
        [ -z "$dns" ] && continue

        resolved=$(dig +short "$dns" 2>/dev/null) || continue
        if echo "$resolved" | grep -qF "$TARGET_IP"; then
          echo "[CLB] Account: ${account_name} (${account_id}) | Region: ${region}"
          echo "      Name: ${elb_name} | DNS: ${dns}"
          found=$((found + 1))
        fi
      done
    fi

    # Check ALB/NLB
    elbv2_arns=$(aws elbv2 describe-load-balancers --region "$region" \
      --query 'LoadBalancers[].[LoadBalancerArn,DNSName,LoadBalancerName,Type]' \
      --output text 2>/dev/null) || true

    if [ -n "$elbv2_arns" ] && [ "$elbv2_arns" != "None" ]; then
      while IFS=$'\t' read -r lb_arn lb_dns lb_name lb_type; do
        [ -z "$lb_arn" ] && continue
        [ -z "$lb_dns" ] && continue

        resolved=$(dig +short "$lb_dns" 2>/dev/null) || continue
        if echo "$resolved" | grep -qF "$TARGET_IP"; then
          echo "[${lb_type^^}] Account: ${account_name} (${account_id}) | Region: ${region}"
          echo "      Name: ${lb_name} | DNS: ${lb_dns}"
          found=$((found + 1))
        fi
      done <<< "$elbv2_arns"
    fi

  done <<< "$region_list"

  unset AWS_ACCESS_KEY_ID AWS_SECRET_ACCESS_KEY AWS_SESSION_TOKEN
done <<< "$accounts"

echo ""
if [ "$found" -eq 0 ]; then
  echo "IP ${TARGET_IP} not found in any account."
else
  echo "Total matches: ${found}"
fi
