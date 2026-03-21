#!/bin/bash
# create-user.sh - Create an OpsAgent admin user
# Usage: ./scripts/create-user.sh [config-path]
set -euo pipefail

CONFIG_PATH="${1:-knowledge/_config/users.yaml}"

echo "=== OpsAgent User Creator ==="
echo ""

read -p "Username: " USERNAME
if [ -z "$USERNAME" ]; then
  echo "Error: Username is required"
  exit 1
fi

read -sp "Password: " PASSWORD
echo ""
if [ -z "$PASSWORD" ]; then
  echo "Error: Password is required"
  exit 1
fi

echo "Roles: super_admin, tenant_admin"
read -p "Role [super_admin]: " ROLE
ROLE="${ROLE:-super_admin}"

TENANT_ID=""
if [ "$ROLE" = "tenant_admin" ]; then
  read -p "Tenant ID: " TENANT_ID
  if [ -z "$TENANT_ID" ]; then
    echo "Error: Tenant ID is required for tenant_admin role"
    exit 1
  fi
fi

# Generate bcrypt hash
HASH=$(node -e "require('bcryptjs').hash(process.argv[1], 10).then(h => console.log(h))" "$PASSWORD")

# Ensure config directory exists
mkdir -p "$(dirname "$CONFIG_PATH")"

# If file doesn't exist, create it
if [ ! -f "$CONFIG_PATH" ]; then
  echo "users: []" > "$CONFIG_PATH"
fi

# Append user entry using Node.js (safe YAML manipulation)
node -e "
const fs = require('fs');
const yaml = require('js-yaml');

const configPath = process.argv[1];
const content = fs.readFileSync(configPath, 'utf-8');
const config = yaml.load(content) || { users: [] };
if (!config.users) config.users = [];

const username = process.argv[2];
const hash = process.argv[3];
const role = process.argv[4];
const tenantId = process.argv[5] || undefined;

// Check for duplicate username
if (config.users.some(u => u.username === username)) {
  console.error('Error: User \"' + username + '\" already exists');
  process.exit(1);
}

const user = { username, password_hash: hash, role };
if (tenantId) user.tenant_id = tenantId;
config.users.push(user);

fs.writeFileSync(configPath, yaml.dump(config, { lineWidth: 120, noRefs: true }), 'utf-8');
console.log('User \"' + username + '\" created successfully (' + role + ')');
" "$CONFIG_PATH" "$USERNAME" "$HASH" "$ROLE" "$TENANT_ID"
