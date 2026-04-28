#!/usr/bin/env bash
# Render *.yaml.template -> k8s/rendered/ by substituting test values.
set -euo pipefail

cd "$(dirname "$0")/../.."

SRC=k8s/application
DST=k8s/rendered
mkdir -p "$DST"
rm -f "$DST"/*.yaml

# Test substitution values (non-prod, schema-complete)
export NAMESPACE=ops
export IMAGE_TAG=test
export BACKEND_IMAGE=123456789012.dkr.ecr.us-east-1.amazonaws.com/ops-backend
export FRONTEND_IMAGE=123456789012.dkr.ecr.us-east-1.amazonaws.com/ops-frontend
export CERT_ARN=arn:aws:acm:us-east-1:123456789012:certificate/test
export DB_SECRET_ARN=arn:aws:secretsmanager:us-east-1:123456789012:secret:ops-test
export DB_HOST=ops-test.cluster-xyz.us-east-1.rds.amazonaws.com
export DB_NAME=ops
export DOMAIN=test.example.com
export JWT_SECRET_ARN=arn:aws:secretsmanager:us-east-1:123456789012:secret:jwt-test
export STORAGE_CLASS_KMS_KEY_ID=alias/test
export BACKEND_HPA_MIN=2
export BACKEND_HPA_MAX=10
export FRONTEND_HPA_MIN=2
export FRONTEND_HPA_MAX=6
export BACKEND_CPU_REQUEST=250m
export BACKEND_MEMORY_REQUEST=512Mi
export BACKEND_CPU_LIMIT=1000m
export BACKEND_MEMORY_LIMIT=1Gi
export FRONTEND_CPU_REQUEST=100m
export FRONTEND_MEMORY_REQUEST=256Mi
export FRONTEND_CPU_LIMIT=500m
export FRONTEND_MEMORY_LIMIT=512Mi
export AWS_ACCOUNT_ID=123456789012
export AWS_REGION=us-east-1

# Render templates + copy static yaml
for f in "$SRC"/*.yaml.template "$SRC"/*.yaml; do
  [ -e "$f" ] || continue
  name=$(basename "$f" .template)
  envsubst < "$f" > "$DST/$name"
done

echo "Rendered $(ls "$DST"/*.yaml | wc -l | tr -d ' ') manifests → $DST"
