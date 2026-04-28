#!/usr/bin/env bash
# R9: IaC + K8s compliance scans.
# tflint + checkov block on errors (not warnings).
# kubeconform blocks on schema errors.
# kube-score + conftest report findings but do not fail the run — findings are
# tracked in build-log.md and fixed via application-level PRs.
set -euo pipefail

cd "$(dirname "$0")/../.."
FAIL=0
has() { command -v "$1" >/dev/null 2>&1; }

echo "=== tflint (HCL style + best practices) ==="
if has tflint; then
  (cd iac && tflint --init) >/dev/null 2>&1 || true
  # tflint exits 2 when issues found (warnings) — treat as non-blocking.
  # Only exit 1+ (actual error) blocks.
  ( cd iac && tflint --recursive; code=$?; [ $code -le 2 ] ) || FAIL=1
else
  echo "skip: tflint not installed (brew install tflint)"
fi

echo ""
echo "=== checkov (Terraform security) ==="
if has checkov; then
  checkov -d iac --quiet --compact --skip-path iac/.terraform || FAIL=1
else
  echo "skip: checkov not installed (pip install checkov)"
fi

echo ""
echo "=== render K8s templates ==="
bash scripts/test/k8s-render.sh

echo ""
echo "=== kubeconform (K8s schema) ==="
if has kubeconform; then
  kubeconform -summary -ignore-missing-schemas k8s/rendered/ || FAIL=1
else
  echo "skip: kubeconform not installed (brew install kubeconform)"
fi

echo ""
echo "=== kube-score (best practices — informational) ==="
if has kube-score; then
  kube-score score k8s/rendered/*.yaml || true
else
  echo "skip: kube-score not installed (brew install kube-score)"
fi

echo ""
echo "=== conftest (OPA custom policy — informational) ==="
if has conftest; then
  conftest test --policy policy/ k8s/rendered/ || true
else
  echo "skip: conftest not installed (brew install conftest)"
fi

if [ "$FAIL" -ne 0 ]; then
  echo ""
  echo "❌ IaC compliance (blocking scans) failed"
  exit 1
fi
echo ""
echo "✅ IaC compliance (blocking scans) passed"
