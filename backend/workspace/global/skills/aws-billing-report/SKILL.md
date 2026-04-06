---
name: aws-billing-report
description: Generate professional AWS billing Excel reports with detailed monthly cost breakdowns matching AWS Bills page exactly, including Data Transfer and all usage types. Use when users request AWS billing reports, cost exports, monthly cost reports, or mention keywords like "aws billing", "aws费用", "billing report", "生成AWS账单报表", "导出AWS费用". Uses Cost and Usage Report (CUR) for most detailed billing data.
---

# AWS Billing Report

## Overview

Generate professionally formatted Excel reports with **detailed** AWS billing data that matches the AWS Bills page exactly:
- **Sheet1**: Service-level summary with monthly costs
- **Sheet2**: Detailed drill-down with three-level hierarchy (Service → Region → Usage Type)
- Complete breakdown including Data Transfer, EC2 instances, and all usage types
- Year-over-year comparison with growth percentages
- Microsoft 365 professional styling (Aptos font, light gray headers)

**Data Source:** Cost and Usage Report (CUR) - provides the most detailed billing information available from AWS.

## Platform Detection

**IMPORTANT**: Before starting, check if you are running inside the OpenOps platform by looking for these signals in your system prompt:
- "You are OpenOps AI" or "CRITICAL ENVIRONMENT RULES"
- "Available Cloud Accounts" section with account IDs and regions
- A WORKSPACE path is specified

### If running inside OpenOps → **Auto mode**

When inside OpenOps, most parameters are already available:
- **Auth**: Use default credential chain — credentials are injected via environment variables. Do NOT use `--profile`.
- **Accounts**: Extract from the "Available Cloud Accounts" section in the system prompt.
- **Output path**: Use the WORKSPACE path from system prompt: `<WORKSPACE>/<YYYY-MM-DD>/aws_billing_<account>_<year>.xlsx`. Create the date directory first with `mkdir -p`.
- **Script path**: Skills are symlinked at `.claude/skills/aws-billing-report/scripts/` relative to the working directory.

**Ask the user for the following before proceeding:**
1. Which account to generate the report for (if multiple accounts available)
2. Which year (default: current year)
3. Which month(s) — present options: specific month (e.g. "3" for March), a range (e.g. "1-3" for Jan–Mar), or "all" for the full year

**CRITICAL: You MUST wait for the user's response before proceeding. Do NOT assume defaults for month selection. This is a mandatory interaction point.**

Then proceed to Step 2.

### If running standalone → **Interactive mode**

Ask the user for the following information if not provided:

1. **AWS Account ID** (required): 12-digit AWS account ID
2. **AWS Profile** (optional): `--profile <name>`, or uses default credentials
3. **Year** for the report (default: current year)
4. **Month(s)** — specific month, range, or "all"
5. **Output path** for the Excel report

**CRITICAL: You MUST wait for the user's response before proceeding to Step 2. Do NOT run fetch and report generation in the same turn as the questions.**

Then proceed to Step 2.

## Workflow

### Step 1: Resolve Script Path

**Resolve SCRIPT_PATH**: The scripts live in the `scripts/` directory where this SKILL.md is located:
- **OpenOps mode**: `SCRIPT_PATH=".claude/skills/aws-billing-report/scripts"`
- **Standalone mode**: Use the absolute path, e.g. `SCRIPT_PATH="$HOME/.agents/skills/aws-billing-report/scripts"` or wherever installed.

### Step 2: Check and Setup CUR

Check if CUR is already configured:

```bash
python3 <SCRIPT_PATH>/setup_cur.py --check-only [--profile <PROFILE>]
```

**If CUR is configured:** → Proceed to Step 3.

**If CUR is NOT configured:**
```bash
python3 <SCRIPT_PATH>/setup_cur.py [--profile <PROFILE>] [--account-id <ACCOUNT_ID>]
```

Setup creates: S3 bucket `aws-cur-{account_id}`, CUR report definition (Parquet, daily).

**Important:** ⏰ First CUR report takes **24 hours** to generate. Inform the user and stop if this is the first setup.

### Step 3: Fetch Billing Data

```bash
python3 <SCRIPT_PATH>/fetch_aws_billing.py <ACCOUNT_ID> <YEAR> --months <MONTHS> [--profile <PROFILE>] > billing_<YEAR>.json 2>/dev/null
```

`--months` options: single (`3`), range (`1-6`), comma-separated (`1,3,6`), or `all`. Default: current month.

For multi-year reports, fetch each year separately.

### Step 4: Generate Excel Report

```bash
python3 <SCRIPT_PATH>/generate_excel_report.py <OUTPUT_PATH> billing_<YEAR1>.json [billing_<YEAR2>.json ...]
```

The generated report contains:

**Sheet1 - Service Summary:**
- Service-level costs for each month
- Clean service names matching AWS Bills page
- Total row with auto-calculated formulas

**Sheet2 - Usage Details:**
- Three-level drill-down: Service → Region → Usage Type
- Description, Quantity with unit, Rate with unit, Cost

### Step 5: Deliver Report and Cleanup

1. Verify the file was created successfully
2. Display a summary (total cost, top services, service count)
3. Clean up temporary JSON files: `rm -f billing_*.json`

## Prerequisites

Automatically handled during execution:
1. **Python Packages**: boto3, pyarrow, pandas, openpyxl — install if missing:
   ```bash
   pip install boto3 pyarrow pandas openpyxl
   ```
2. **AWS Permissions**: S3 (CreateBucket, PutBucketPolicy, ListBucket, GetObject), CUR (PutReportDefinition, DescribeReportDefinitions)

## Error Handling

- **Missing credentials**: Check profile, suggest `aws sso login`
- **CUR not configured**: Auto-run setup, inform about 24-hour wait
- **Missing packages**: Display pip install command
- **No CUR data**: Check if 24 hours passed since setup
