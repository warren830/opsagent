# AWS Billing Report Skill

Generate professional AWS billing Excel reports with detailed cost breakdowns that **exactly match the AWS Bills page**, including Data Transfer and all usage types.

## Features

- ✅ **Two-sheet Excel report**:
  - **Sheet1**: Service-level summary with monthly costs
  - **Sheet2**: Detailed drill-down with three-level hierarchy (Service → Region → Usage Type)
- ✅ **Detailed billing data** from AWS Cost and Usage Report (CUR)
- ✅ **Complete breakdown** including Data Transfer, EC2 instances, S3 storage, and all usage types
- ✅ **Matches AWS Bills page format** exactly with drill-down capabilities
- ✅ **Professional Excel styling** with Microsoft 365 Aptos font
- ✅ **Year-over-year comparison** with growth percentages
- ✅ **Fully automated** - all setup and generation through Claude Code interaction
- ✅ **99.94% accuracy** - data matches AWS Bills within 0.06%

## Quick Start

### 1. Prerequisites

Before using this skill, ensure you have:

1. **Claude Code CLI** installed ([Download](https://claude.ai/download))
2. **AWS credentials** configured with billing access
3. **Python packages**: `boto3`, `pyarrow`, `pandas`, `openpyxl`

<details>
<summary><strong>First-time Bedrock Users</strong></summary>

If this is your first time using Anthropic models on Amazon Bedrock:
1. Go to [Amazon Bedrock Console](https://console.aws.amazon.com/bedrock/)
2. Select **Chat/Text playground**
3. Choose any Anthropic model
4. Fill out the one-time use case form
5. Access is granted immediately

</details>

<details>
<summary><strong>Configure Claude Code with Bedrock (Recommended)</strong></summary>

**Create Bedrock API Key:**
1. Navigate to **Amazon Bedrock → API keys**
2. Click **Create API key**
3. Save the API key (you won't see it again)

**Set environment variables:**
```bash
export CLAUDE_CODE_USE_BEDROCK=1
export AWS_BEARER_TOKEN_BEDROCK=your-bedrock-api-key
export AWS_REGION=us-east-1  # or your preferred region
```

**Add to shell profile for persistence:**
```bash
# For bash
echo 'export CLAUDE_CODE_USE_BEDROCK=1' >> ~/.bashrc
echo 'export AWS_BEARER_TOKEN_BEDROCK=your-bedrock-api-key' >> ~/.bashrc
echo 'export AWS_REGION=us-east-1' >> ~/.bashrc
source ~/.bashrc

# For zsh
echo 'export CLAUDE_CODE_USE_BEDROCK=1' >> ~/.zshrc
echo 'export AWS_BEARER_TOKEN_BEDROCK=your-bedrock-api-key' >> ~/.zshrc
echo 'export AWS_REGION=us-east-1' >> ~/.zshrc
source ~/.zshrc
```

For detailed setup instructions, see [Claude Code on Amazon Bedrock](https://code.claude.com/docs/en/amazon-bedrock).

</details>

### 2. Install the Skill

```bash
# Option A: Install from .skill file
claude skill install aws-billing-report.skill

# Option B: Install from directory
cp -r aws-billing-report ~/.claude/skills/

# Option C: Install from GitHub
npx skills add https://github.com/koljahuang/aws-billing-report.git

# Verify installation
claude skill list
```

### 3. Generate Your First Report

**Start Claude Code:**
```bash
claude
```

**Trigger the skill with any of these phrases:**
- "Generate AWS billing report"
- "aws billing"
- "kolya billing"
- "生成AWS账单报表"

**Example conversation:**

```
You: Generate AWS billing report for 2026

Claude: To generate the AWS billing report with detailed breakdown (including Data Transfer), I need:
1. AWS account ID (e.g., 123456789012)
2. AWS profile name (optional - will use AWS_PROFILE or default credentials if not provided)
3. Year(s) for the report: 2026 ✓
4. Output path - where to save the Excel file?
   (e.g., /Users/username/Documents/aws_billing_2026.xlsx)
   Or just specify a directory, I'll use: aws_billing_2026.xlsx as filename

You: 612674025488; /Users/username/

Claude: ✅ Checking CUR configuration...
✅ Fetching billing data from CUR...
✅ Generating Excel report...
✅ Report saved to /Users/username/aws_billing_2026.xlsx

Report generated successfully!
📊 Total Cost (Feb): $675.06 USD
📋 Sheet1: 22 services
🔍 Sheet2: 105 detailed usage records
```

**That's it!** Claude Code handles everything automatically:
- ✅ Checks if CUR is configured
- ✅ Sets up CUR if needed (first-time only)
- ✅ Fetches detailed billing data
- ✅ Generates professionally formatted Excel report
- ✅ Displays summary of costs

## Report Structure

### Sheet1 - Service Summary

Service-level costs with monthly breakdown:

| Description | Jan | Feb | Mar | ... | Dec | Total |
|------------|-----|-----|-----|-----|-----|-------|
| OpenSearch Service | 0.00 | 126.17 | 0.00 | ... | 0.00 | =SUM(B2:M2) |
| Elastic Container Service | 0.00 | 104.39 | 0.00 | ... | 0.00 | =SUM(B3:M3) |
| QuickSight | 0.00 | 99.48 | 0.00 | ... | 0.00 | =SUM(B4:M4) |
| Data Transfer | 0.00 | 0.34 | 0.00 | ... | 0.00 | =SUM(B5:M5) |
| **Total** | **0.00** | **675.06** | **0.00** | ... | **0.00** | **=SUM(N2:N22)** |

**Features:**
- Clean service names matching AWS Bills page
- Auto-calculated totals with Excel formulas
- Professional formatting with Aptos font

### Sheet2 - Usage Details (Drill-Down)

Three-level hierarchy exactly matching AWS Bills page:

```
─ Data Transfer ($0.34)
  └─ US West (Oregon)                                      $0.34
      └─ AWS Data Transfer USW2-DataTransfer-Regional-Bytes
          $0.010 per GB - regional data transfer...    18.387 GB    $0.18
      └─ AWS Data Transfer USW2-DataTransfer-Regional-Bytes
          $0.010 per GB - regional data transfer...    9.969 GB     $0.10
      └─ AWS Data Transfer USW2-DataProcessing-Bytes
          $0.50 per GB - custom log data ingested...   0.085 GB     $0.04

─ OpenSearch Service ($126.17)
  └─ US West (Oregon)                                      $126.17
      └─ USW2-IndexingOCU
          $0.24 per OCU-hours for IndexingOCU...       192.000      $46.08
      └─ USW2-SearchOCU
          $0.24 per OCU-hours for SearchOCU...         192.000      $46.08
```

**Features:**
- Service → Region → Usage Type hierarchy
- Full descriptions from AWS Bills
- Quantity with units (GB, hours, requests, etc.)
- Rate with units (per GB, per hour, etc.)
- Cost for each usage type

## How It Works

### Fully Automated Process

When you request a billing report, Claude Code automatically:

1. **Collects Information** - Asks for:
   - AWS account ID
   - AWS profile name (optional - uses environment variable or default credentials if not provided)
   - Year(s) for the report
   - **Output path** (where to save the Excel file)
2. **Checks CUR Setup** - Verifies Cost and Usage Report is configured
3. **Sets Up CUR (if needed)** - One-time 5-minute setup + 24-hour wait for first data
4. **Fetches Billing Data** - Reads from S3 CUR location, processes all usage types
5. **Generates Excel Report** - Creates professionally formatted two-sheet workbook
6. **Delivers Report** - Saves to **your specified location** and displays summary
7. **Cleans Up** - Automatically removes temporary JSON files, keeping only the final Excel report

### Data Source: Cost and Usage Report (CUR)

**What is CUR?**

**CUR (Cost and Usage Report)** is AWS's most comprehensive billing data source. It provides:
- Detailed usage information for all AWS services
- Granular data: usage types, quantities, rates, costs
- Regional breakdown
- Resource IDs and tags
- Line-item level detail matching the AWS Bills page

**Key Characteristics:**

- **Format**: Parquet files in S3
- **Update frequency**: Daily (updates current month automatically)
- **Storage location**: `s3://aws-cur-{account_id}/cur/billing-report-detailed/`
- **Data detail**: Most comprehensive billing data available from AWS
- **Includes**: All services, regions, usage types, descriptions, quantities, rates

**Important: Data Availability**

⚠️ **CUR only records data from the time it was configured forward** - historical data before CUR setup is NOT available.

**Example:**
- If you configure CUR on February 5, 2026
- Your report will show data starting from February 2026
- January 2026 and earlier months will have no data (shows $0.00)
- From March 2026 onward, you'll have complete monthly data

**Why do I only see some months with data?**

This happens because:
1. **First-time setup**: CUR was just configured, so only current month has data
2. **CUR cannot backfill**: Historical billing data before CUR setup is not captured
3. **Daily updates**: Once configured, CUR updates automatically every day

**How to get historical billing data:**

For months before CUR was configured, you can:
- Use **AWS Cost Explorer** - provides historical trends and graphs
- View **AWS Bills page** - shows monthly totals and service-level summaries
- Download **AWS Bills (PDF/CSV)** - basic billing information

However, none of these provide the same level of detail as CUR (usage types, quantities, rates, drill-down hierarchy).

## Requirements

### AWS Permissions

Your AWS credentials need permissions for:

**For report generation:**
- S3: `ListBucket`, `GetObject` (read CUR data)
- CUR: `DescribeReportDefinitions` (check CUR status)

**For first-time CUR setup (one-time):**
- S3: `CreateBucket`, `PutBucketPolicy`
- CUR: `PutReportDefinition`

### Python Dependencies

```bash
pip install boto3 pyarrow pandas openpyxl
```

Claude Code will check and prompt you if any packages are missing.

## Advanced Usage

### Multi-Year Comparison

Generate year-over-year comparison reports:

```
You: Generate AWS billing report for 2024 and 2025

Claude: [Fetches data for both years]
        [Generates report with year-over-year growth percentages]
```

The report will include:
- Separate data rows for each year
- Year-over-year growth percentages
- Side-by-side cost comparison

### Custom Output Path

During the conversation, Claude will ask where to save the report. You can specify:

**Option 1: Full path with filename**
```
You: /Users/username/Documents/aws_billing_2026.xlsx
```

**Option 2: Directory only** (Claude will use `aws_billing_<year>.xlsx` as filename)
```
You: /Users/username/Documents/
```

**Option 3: Just the home directory**
```
You: ~/
```

The output path is **always specified during the conversation**, not fixed in advance.

### Multiple AWS Accounts

Generate reports for different accounts:

```
You: Generate billing report for account 111111111111
     [complete first report]

You: Generate billing report for account 222222222222
     [complete second report]
```

Each account needs its own CUR configuration.

## Troubleshooting

### CUR Not Configured

**Issue:** First time using the skill

**Solution:** Claude Code will automatically:
1. Create S3 bucket
2. Configure bucket policy
3. Create CUR report definition
4. Inform you to wait 24 hours for first data

**Next steps:** Run the skill again after 24 hours

### Missing Dependencies

**Issue:** `ModuleNotFoundError: No module named 'pyarrow'`

**Solution:**
```bash
pip install boto3 pyarrow pandas openpyxl
```

### AWS Credentials Not Found

**Issue:** Cannot access AWS resources

**Solution:** Ensure AWS credentials are configured:
```bash
# Option 1: Use AWS CLI
aws configure

# Option 2: Use AWS profile
export AWS_PROFILE=your-profile

# Option 3: Set credentials directly
export AWS_ACCESS_KEY_ID=your-key
export AWS_SECRET_ACCESS_KEY=your-secret
```

### Skill Not Triggering

**Issue:** Claude doesn't recognize billing request

**Solution:**
1. Check installation: `claude skill list`
2. Try explicit trigger phrases: "aws billing" or "kolya billing"
3. Reinstall: `claude skill install aws-billing-report.skill`

### Data Accuracy Differences

**Issue:** Report shows slightly different amounts than AWS Console

**Solution:** Minor differences (<0.1%) are normal due to:
- Rounding differences
- CUR data processing timing
- Hourly vs daily aggregation

Report accuracy is typically 99.94% compared to AWS Bills page.

## Cost Information

### S3 Storage (CUR Data)

- **Storage cost**: ~$0.023 per GB per month
- **Typical CUR size**: 10-100 MB per month
- **Estimated cost**: $0.01 - $0.10 per month

### Bedrock API (Report Generation)

Typical usage per report:
- ~5,000 input tokens
- ~2,000 output tokens
- **Cost**: $0.03 - $0.15 per report (depending on model)

## FAQ

**Q: How often is CUR data updated?**
A: Daily. Reports contain billing data up to the previous day.

**Q: Can I use this for multiple AWS accounts?**
A: Yes! Configure CUR for each account separately.

**Q: How far back can I get billing data?**
A: CUR only records data from when it was configured. Historical data before CUR setup is not available. For example, if you configured CUR in February 2026, you'll only have data from February 2026 onward. See the "Data Source: Cost and Usage Report (CUR)" section for alternatives to access historical billing data.

**Q: Why does my report show $0.00 for some months?**
A: This happens when CUR was configured after those months. CUR cannot backfill historical data - it only records from the configuration date forward. Months before CUR setup will show no data ($0.00).

**Q: Does this work with AWS Organizations?**
A: Yes, configure CUR in the management account or use consolidated billing.

**Q: Can I automate report generation?**
A: Yes, you can script Claude Code commands or schedule the underlying Python scripts via cron/Lambda.

**Q: Why does the first report take 24 hours?**
A: AWS needs time to generate the first CUR report after initial configuration. Subsequent reports update daily.

**Q: Is my data secure?**
A: Yes. Data stays in your AWS account S3 bucket. Claude Code only reads billing data to generate reports.

## Technical Details

For developers and advanced users, see [SKILL.md](SKILL.md) for:
- Detailed script documentation
- CUR configuration details
- JSON data format
- Error handling
- Customization options

## Support

For issues or questions:
1. Check the [Troubleshooting](#troubleshooting) section
2. Review [SKILL.md](SKILL.md) for detailed documentation
3. Check [AWS Billing documentation](https://docs.aws.amazon.com/cur/)
4. Check [Claude Code documentation](https://code.claude.com/docs/)

## License

This skill is provided as-is for use with Claude Code.

## Acknowledgments

- Uses AWS Cost and Usage Report (CUR) for detailed billing data
- Built for Claude Code CLI
- Generates reports using openpyxl with Microsoft 365 styling
