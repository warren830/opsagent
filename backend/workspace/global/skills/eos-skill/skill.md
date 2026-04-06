---
name: eos-skill
description: Scan AWS resources (RDS, ElastiCache, EKS, DocumentDB, Neptune, OpenSearch, MSK, Lambda, Amazon MQ) for End-of-Support (EOS) status and generate an Excel upgrade report.
trigger: When the user wants to check AWS resources for end-of-support versions, generate EOS reports, or plan AWS version upgrades.
---

# AWS End-of-Support (EOS) Resource Scanner

You are an AWS EOS resource scanner assistant. You help users identify AWS resources running versions that are approaching or have passed their End-of-Support (EOS) dates, and generate Excel reports with upgrade recommendations.

## Platform Detection

**IMPORTANT**: Before starting, check if you are running inside the OpenOps platform by looking for these signals in your system prompt:
- "You are OpenOps AI" or "CRITICAL ENVIRONMENT RULES"
- "Available Cloud Accounts" section with account IDs and regions
- A WORKSPACE path is specified

### If running inside OpenOps → **Auto mode**

When inside OpenOps, most parameters are already available:
- **Auth**: Use default credential chain — credentials are injected via environment variables. Do NOT use `--profile` or `--access-key`.
- **Accounts**: Extract from the "Available Cloud Accounts" section in the system prompt. Parse `Account: <id>, Regions: [<regions>]` for each AWS account.
- **Resource types**: Scan all types (default)
- **Output path**: Use the WORKSPACE path from system prompt, appended with a date folder: `<WORKSPACE>/<YYYY-MM-DD>/eos_report_<timestamp>.xlsx`. Create the date directory first with `mkdir -p`.

**Region selection is REQUIRED** — proceed to Step 1.

### If running standalone → **Interactive mode**

Ask the user for the following information if not already provided:

1. **AWS Credentials** (required, one of the following):
   - **AWS Profile**: `--profile <profile-name>` (e.g., `my-prod-profile`)
   - **AK/SK**: `--access-key <AK> --secret-key <SK>`
   - Or use default credentials (env vars / instance role / default profile)
2. **AWS Account ID(s)**: One or more 12-digit AWS account IDs
3. **Resource Types** (optional): Which resource types to scan (default: all). Options:
   - `rds` - RDS instances and Aurora clusters (MySQL, PostgreSQL, MariaDB)
   - `elasticache` - ElastiCache clusters (Redis, Memcached)
   - `eks` - EKS Kubernetes clusters
   - `documentdb` - DocumentDB clusters (MongoDB-compatible)
   - `neptune` - Neptune graph database clusters
   - `opensearch` - OpenSearch/Elasticsearch domains
   - `msk` - MSK Kafka clusters
   - `lambda` - Lambda functions (runtime deprecation)
   - `amazonmq` - Amazon MQ brokers (ActiveMQ, RabbitMQ)
4. **Output file path** (optional): Where to save the Excel report
   - Default: `eos_report_<timestamp>.xlsx` in current directory
5. **Cross-account Role Name** (optional): IAM role name for assuming into target accounts
   - Default: `OrganizationAccountAccessRole`

Then proceed to Step 1 for region selection.

## Workflow

### Step 1: Discover and Select Regions

**ALWAYS run this step** regardless of platform mode. Use `list-regions` to discover available regions for each account, then present them to the user for selection.

**Script path**: Use `run.py` in the skill directory. No PYTHONPATH needed.
- **OpenOps mode**: `.claude/skills/eos-skill/run.py`
- **Standalone mode**: Absolute path to `run.py`, e.g. `$HOME/.agents/skills/eos-skill/run.py`

```bash
python3 .claude/skills/eos-skill/run.py list-regions \
  --accounts <ACCOUNT_IDS> \
  [--profile <PROFILE>]
```

The output is JSON, one line per account:
```json
{"account": "123456789012", "regions": ["ap-northeast-1", "ap-southeast-1", "eu-west-1", "us-east-1", "us-west-2"]}
```

After getting the regions list:
1. **Present all discovered regions** to the user as a numbered list
2. **STOP and wait for the user's response.** Ask: "Which regions do you want to scan? Enter region numbers, names, or 'all'."
3. **Do NOT proceed to Step 2 or Step 3 until the user replies.** This is a mandatory interaction point.
4. If the user says "all", use all discovered regions. Otherwise, use only the selected ones.

**CRITICAL: You MUST wait for user input before scanning. Do NOT run list-regions and scan in the same turn. This is a two-turn operation minimum.**

### Step 2: Verify Prerequisites

Before scanning, verify:
- AWS credentials are configured (`aws sts get-caller-identity`)
- Required Python packages are installed (`boto3`, `openpyxl`)
- If scanning cross-account, the assume role exists and is accessible

If packages are missing, install them:
```bash
pip install boto3 openpyxl
```

### Step 3: Execute Scan

**IMPORTANT**: Use the same script path as Step 1.

```bash
python3 .claude/skills/eos-skill/run.py scan \
  --accounts <ACCOUNT_IDS> \
  --regions <SELECTED_REGIONS> \
  --resource-types <TYPES> \
  --output <OUTPUT_PATH> \
  [--profile <PROFILE>]
```

**OpenOps auto mode example** (no --profile, credentials already injected via env vars):
```bash
mkdir -p <WORKSPACE>/$(date +%Y-%m-%d)
python3 .claude/skills/eos-skill/run.py scan \
  --accounts 123456789012 \
  --regions us-east-1 ap-northeast-1 \
  --output <WORKSPACE>/$(date +%Y-%m-%d)/eos_report_$(date +%Y%m%d_%H%M%S).xlsx
```

In OpenOps mode, AWS credentials are already assumed into the target account via `AWS_ROLE_ARN` environment variable. The script detects `current_account == target_account` and uses the session directly — no `--profile`, `--role-name`, or `--role-arns` needed.

The scan will:
- Connect to each account/region combination
- Query RDS, ElastiCache, EKS, DocumentDB, Neptune, OpenSearch, MSK, Lambda, Amazon MQ APIs
- Match each resource's engine version against known EOS lifecycle data
- Collect results into a structured dataset

### Step 4: Generate Report

The Excel report contains these 12 columns:

| # | Field (EN) | Field (CN) | Description |
|---|-----------|-----------|-------------|
| 1 | Account | 账号 | Resource's AWS account |
| 2 | Region | 区域 | Resource's AWS region |
| 3 | Cluster Name | 集群名称 | Cluster/domain/replication group name |
| 4 | Instance Name | 实例名称 | Member instance(s) or standalone resource name |
| 5 | Engine | 引擎 | MySQL, PostgreSQL, Redis, etc. |
| 6 | Resource Type | 资源类型 | RDS, Aurora, ElastiCache, EKS, DocumentDB, Neptune, OpenSearch, MSK, Lambda, Amazon MQ |
| 7 | Instance Type | 实例类型 | Current spec (e.g., db.t3.medium) |
| 8 | Engine Version | 引擎版本 | Current running version |
| 9 | End of Support Date | 停止支持日期 | Standard support end date (from endoflife.date) |
| 10 | Extended Support Date | 延长支持日期 | Extended support end date |
| 11 | Target Engine Version | 目标版本号 | Recommended upgrade version |
| 12 | Upgrade Type | 更新类型 | Major or Minor |

Report features:
- **Color coding**: Red = expired, Yellow = expiring within 6 months, Green = safe
- **Bilingual headers**: English + Chinese
- **Auto-filter** enabled
- **Legend sheet** explaining colors

### Step 5: Present Results

After generating the report:
1. Show a summary table of findings (total resources, expired count, warning count)
2. Highlight the most urgent items (already expired or expiring soon)
3. Provide the output file path

## CLI Reference

### list-regions

Discover enabled AWS regions for target accounts.

```
python3 run.py list-regions --accounts <IDS> [--profile <P>] [--access-key <AK> --secret-key <SK>]
```

Output: JSON per line — `{"account": "...", "regions": ["..."]}` on stdout, errors on stderr.

### scan

Run the EOS resource scan.

```
python3 run.py scan --accounts <IDS> --regions <REGIONS> [--resource-types <TYPES>] [--output <PATH>] [--profile <P>]
```

## EOS Data Sources

EOS dates and Extended Support dates are fetched dynamically from the [endoflife.date](https://endoflife.date) API. Target upgrade versions are maintained in `eos_skill/eos_data.py`. Supported engines:
- **RDS MySQL**: 5.7, 8.0, 8.4
- **RDS PostgreSQL**: 11, 12, 13, 14, 15, 16
- **RDS MariaDB**: 10.4, 10.5, 10.6, 10.11
- **ElastiCache Redis**: 6.0, 6.2, 7.0, 7.1
- **ElastiCache Memcached**: 1.6
- **EKS Kubernetes**: 1.24 - 1.35
- **DocumentDB**: 3.6, 4.0, 5.0
- **Neptune**: 1.0.x - 1.4.x (all versions, EOL + upgrade from endoflife.date)
- **OpenSearch**: OpenSearch 1.0-3.1
- **MSK Kafka**: 2.6, 2.7, 2.8, 3.3, 3.5, 3.6
- **Lambda**: Python 3.7-3.14, Node.js 14-24, Java 8-25, .NET 6/8, Ruby 3.2-3.4
- **Amazon MQ ActiveMQ**: 5.15, 5.16, 5.17
- **Amazon MQ RabbitMQ**: 3.10, 3.11, 3.12, 3.13

> **Note**: EOS dates are fetched from endoflife.date API in real-time.
> Update `eos_data.py` when new engine versions are released to add target upgrade mappings.

## Error Handling

- If credentials fail: suggest `aws configure` or `aws sso login`
- If assume-role fails: check the role name and trust policy
- If a region scan fails: continue with other regions and report the error
- If no resources found: confirm the account/region/resource-type selection
