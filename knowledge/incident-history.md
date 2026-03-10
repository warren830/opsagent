# Incident History

## INC-2026-001: E-Commerce Checkout Failure (2026-01-15)

**Impact**: Payment processing down for 45 minutes during peak hours
**Root Cause**: Stripe API key rotation was not propagated to payment-gateway pods
**Resolution**: Updated Secrets Manager, restarted payment-gateway pods
**Action Items**:
- Added automated secret rotation check in daily monitoring
- payment-gateway now reads secrets on each request (not cached at startup)

## INC-2026-002: Data Pipeline Backlog (2026-02-03)

**Impact**: 6-hour delay in data processing, stale recommendation results
**Root Cause**: Flink TaskManager OOM due to state backend growth
**Resolution**: Increased TaskManager memory from 2GB to 4GB, added checkpoint cleanup
**Action Items**:
- Set `state.checkpoints.dir` to S3 (done)
- Added Flink checkpoint monitoring alarm
- Weekly state compaction scheduled

## INC-2026-003: Banking Dual-Write Divergence (2026-02-20)

**Impact**: 142 transactions recorded in Aurora but not in Oracle
**Root Cause**: Oracle connection pool exhaustion during peak hours (ORACLE_POOL_MAX was 10)
**Resolution**: Increased pool to 20, added retry logic
**Action Items**:
- ORACLE_POOL_MAX increased from 10 to 20
- Added connection pool monitoring
- Reconciliation job now runs every 4 hours (was daily)

## INC-2026-004: AML False Positive Spike (2026-03-01)

**Impact**: 500+ false positive alerts in 2 hours, overwhelming compliance team
**Root Cause**: Watchlist S3 file corrupted during update
**Resolution**: Rolled back watchlist to previous version
**Action Items**:
- Added S3 object versioning on compliance-watchlists-prod
- Added watchlist validation step before deployment
- ALERT_THRESHOLD adjusted from 0.70 to 0.75
