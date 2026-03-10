# Legacy Banking Migration Runbook

## Migration Overview

The core banking system (CBS) is being migrated from Oracle Database + IBM Mainframe to AWS (Aurora PostgreSQL + EKS). The migration runs on EKS cluster `legacy-migration` (K8s v1.29) in us-east-1.

**IMPORTANT: This cluster runs K8s v1.29 which reaches end-of-support soon. Upgrade to v1.30+ is planned for Q2 2026.**

### Migration Status

| Service | Status | Strategy | Notes |
|---------|--------|----------|-------|
| account-service | **Completed** | Dual-write → cutover | Fully on Aurora, Oracle read-only backup |
| transaction-service | **In Progress** | Dual-write active | Both Aurora and Oracle receive writes |
| loan-service | **Early Stage** | Read-only proxy | All writes still go to mainframe |
| notification-service | Not Started | - | Still runs on-premises |
| reporting-service | Not Started | - | Still runs on mainframe |

### Legacy System Connections

| Adapter | Connects To | Protocol | Purpose |
|---------|-------------|----------|---------|
| oracle-db-adapter | oracle-legacy.internal.bank.example.com:1521 | Oracle TNS (SID: BANKPROD) | Direct DB access |
| mainframe-gateway | mainframe-gw.internal.bank.example.com:3270 | TN3270E (EBCDIC-037) | Screen scraping + API |
| legacy-txn-adapter | oracle-db-adapter + mainframe-gateway | HTTP REST | Transaction routing |

### Database Map

| Database | Host | Type | Used By |
|----------|------|------|---------|
| core_accounts | accounts-prod.cluster-bank001.us-east-1.rds.amazonaws.com | Aurora PostgreSQL | account-service |
| core_transactions | transactions-prod.cluster-bank002.us-east-1.rds.amazonaws.com | Aurora PostgreSQL | transaction-service |
| audit_log | audit-prod.cluster-bank003.us-east-1.rds.amazonaws.com | Aurora PostgreSQL | compliance |
| BANKPROD | oracle-legacy.internal.bank.example.com:1521 | Oracle (legacy) | oracle-db-adapter |

## Compliance

### PCI-DSS / AML
- AML scanner runs in `compliance` namespace with real-time screening
- Alert threshold: 0.75 (scores above trigger investigation)
- Alerts go to SNS: aml-alerts
- KYC verification via Jumio API
- All transactions audited in DynamoDB: compliance-audit-log

### Network Isolation
- core-banking namespace: NetworkPolicy restricts ingress to banking-gateway and core-banking only
- banking-api-gw: requires mTLS + OAuth2 (issuer: auth.bank.example.com)
- Internal only: banking-ingress uses `internal` ALB scheme

## Troubleshooting

### Dual-Write Inconsistency
1. Check daily-reconciliation CronJob results: `kubectl logs -n core-banking job/daily-reconciliation-<date>`
2. Compare record counts: Aurora vs Oracle
3. Check transaction-service logs for DUAL_WRITE errors
4. If divergence found: Oracle is source-of-truth during migration

### Mainframe Connection Timeout
1. Check mainframe-gateway logs: `kubectl logs -n legacy-adapter -l app=mainframe-gateway`
2. Default timeout: 30s (MAINFRAME_TIMEOUT_MS)
3. Check TN3270E session pool: CONNECTION_POOL_SIZE=10
4. Verify VPN tunnel to on-premises (network team)

### Oracle Connection Pool Exhaustion
1. Check oracle-db-adapter: ORACLE_POOL_MAX=20
2. Monitor active sessions: `kubectl exec -n legacy-adapter <pod> -- ...`
3. If pool full, scale up oracle-db-adapter replicas

## On-Call

- Core Banking: @core-banking-team (Slack #banking-oncall)
- Migration Lead: Warren Chen
- Compliance: @compliance-team (escalation required for AML alerts)
