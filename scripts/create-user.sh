#!/usr/bin/env bash
#
# Create a user via the Ops API
# Usage: ./scripts/create-user.sh [admin|tenant_admin] [username]
#
set -euo pipefail

API_BASE="${API_BASE:-http://localhost:3080}"
ROLE="${1:-super_admin}"
USERNAME="${2:-admin}"

echo "Creating user: $USERNAME (role: $ROLE)"
echo -n "Password: "
read -rs PASSWORD
echo ""

if [[ "$ROLE" == "tenant_admin" ]]; then
  echo -n "Tenant ID: "
  read -r TENANT_ID
  BODY=$(cat <<EOF
{
  "username": "$USERNAME",
  "password": "$PASSWORD",
  "role": "$ROLE",
  "tenant_id": "$TENANT_ID"
}
EOF
)
else
  BODY=$(cat <<EOF
{
  "username": "$USERNAME",
  "password": "$PASSWORD",
  "role": "$ROLE"
}
EOF
)
fi

# Login as admin to get token
echo -n "Admin password (for auth): "
read -rs ADMIN_PW
echo ""

TOKEN=$(curl -s -X POST "$API_BASE/api/auth/login" \
  -H "Content-Type: application/json" \
  -d "{\"username\":\"admin\",\"password\":\"$ADMIN_PW\"}" | jq -r '.token')

if [[ -z "$TOKEN" || "$TOKEN" == "null" ]]; then
  echo "Failed to authenticate as admin"
  exit 1
fi

curl -s -X POST "$API_BASE/api/users" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d "$BODY" | jq .

echo "User created successfully!"
