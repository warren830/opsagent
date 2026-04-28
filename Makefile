.PHONY: help test-all test-backend test-frontend test-e2e test-security test-iac test-perf coverage clean

help:
	@echo "Testing targets:"
	@echo "  test-all       Run all test dimensions sequentially"
	@echo "  test-backend   Rust: cargo test + tarpaulin coverage"
	@echo "  test-frontend  Nuxt: vitest + coverage"
	@echo "  test-e2e       Playwright: chromium/firefox/webkit + a11y"
	@echo "  test-security  cargo-audit + deny + npm-audit + trivy + gitleaks + semgrep"
	@echo "  test-iac       tflint + checkov + kubeconform + kube-score + conftest"
	@echo "  test-perf      k6 smoke + criterion bench"
	@echo "  coverage       HTML coverage reports"
	@echo "  clean          Remove generated artifacts"

test-all: test-backend test-frontend test-e2e test-security test-iac

test-backend:
	cd backend && cargo test --all

test-backend-coverage:
	cd backend && cargo tarpaulin --workspace --out Html --out Xml --output-dir ../target/coverage

test-frontend:
	cd frontend && npm run test

test-frontend-coverage:
	cd frontend && npm run test:coverage

test-e2e:
	cd e2e && npx playwright test

test-security:
	bash scripts/test/run-security.sh

test-iac:
	bash scripts/test/run-iac.sh

test-perf:
	k6 run scripts/perf/smoke.js

coverage: test-backend-coverage test-frontend-coverage
	@echo "Coverage reports: target/coverage/ and frontend/coverage/"

k8s-render:
	bash scripts/test/k8s-render.sh

clean:
	rm -rf target/coverage frontend/coverage e2e/test-results e2e/report k8s/rendered
