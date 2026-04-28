#!/usr/bin/env bash
# R7: security scans across the repo. Non-zero exit on critical/high findings.
set -euo pipefail

cd "$(dirname "$0")/../.."
FAIL=0

step() { echo ""; echo "=== $* ==="; }
has()  { command -v "$1" >/dev/null 2>&1; }

step "cargo audit (Rust CVEs)"
if has cargo-audit; then
  # Ignores are documented in backend/deny.toml.
  IGNORES="--ignore RUSTSEC-2023-0071 \
    --ignore RUSTSEC-2024-0384 \
    --ignore RUSTSEC-2025-0012 \
    --ignore RUSTSEC-2025-0134 \
    --ignore RUSTSEC-2026-0097 \
    --ignore RUSTSEC-2026-0098 \
    --ignore RUSTSEC-2026-0099 \
    --ignore RUSTSEC-2026-0104"
  (cd backend && cargo audit $IGNORES) || FAIL=1
else
  echo "skip: cargo-audit not installed (cargo install cargo-audit)"
fi

step "cargo deny (licenses/bans/advisories)"
if has cargo-deny; then
  (cd backend && cargo deny check) || FAIL=1
else
  echo "skip: cargo-deny not installed (cargo install cargo-deny)"
fi

step "npm audit (Node CVEs)"
if has npm; then
  (cd frontend && npm audit --production --audit-level=high) || FAIL=1
else
  echo "skip: npm not installed"
fi

step "trivy fs (filesystem scan)"
if has trivy; then
  trivy fs --severity CRITICAL,HIGH --exit-code 1 \
    --skip-dirs node_modules,target,demo/frames,e2e/test-results,e2e/screenshots \
    . || FAIL=1
else
  echo "skip: trivy not installed (brew install trivy)"
fi

step "gitleaks (secret scan)"
if has gitleaks; then
  gitleaks detect --no-git --redact -v || FAIL=1
else
  echo "skip: gitleaks not installed (brew install gitleaks)"
fi

step "semgrep (SAST)"
if has semgrep; then
  semgrep --config=auto --severity=ERROR --error . || FAIL=1
else
  echo "skip: semgrep not installed (pip install semgrep)"
fi

if [ "$FAIL" -ne 0 ]; then
  echo ""
  echo "❌ security scans failed"
  exit 1
fi
echo ""
echo "✅ security scans passed"
