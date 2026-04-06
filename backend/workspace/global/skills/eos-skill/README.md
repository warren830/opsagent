[English](README.md) | [中文](README.zh-CN.md)

# EOS Skill - AWS End-of-Support Resource Scanner

Scan AWS resources for End-of-Support (EOS) status and generate an Excel upgrade report.

## Features

- Scan RDS instances & Aurora clusters (MySQL, PostgreSQL, MariaDB)
- Scan ElastiCache clusters (Redis, Memcached)
- Scan EKS Kubernetes clusters
- Scan DocumentDB clusters (MongoDB-compatible)
- Scan Neptune graph database clusters
- Scan OpenSearch / Elasticsearch domains
- Scan MSK Kafka clusters
- Scan Lambda functions for deprecated runtimes
- Scan Amazon MQ brokers (ActiveMQ, RabbitMQ)
- Color-coded Excel report (Red=expired, Yellow=warning, Green=safe)
- Bilingual headers (English + Chinese)
- Cross-account scanning via IAM role assumption

## Installation

### Install via npx

```bash
npx skills add https://github.com/koljahuang/eos-skill.git
```

When prompted, select the agent directory:

```
── Additional agents ──────────────────────────────
❯ ● Kiro CLI (.kiro/skills)
```

The skill will be installed to `.kiro/skills/eos-skill/`.

### Install manually

```bash
# Clone to .kiro/skills directory
mkdir -p .kiro/skills
git clone https://github.com/koljahuang/eos-skill.git .kiro/skills/eos-skill

# Install dependencies
pip install boto3 openpyxl
```

### Use with Kiro

1. Open Kiro IDE
2. The skill is automatically loaded from `.kiro/skills/eos-skill/`
3. In conversation, type `eos report` to start scanning

## Usage

### Method 1: Claude Code / Kiro Skill

In conversation, say:

```
eos report
```

Claude will ask for:
1. AWS credentials (profile or AK/SK)
2. Account ID(s)
3. Region(s)
4. Resource types

### Method 2: Command Line

#### Basic - single region

```bash
python -m eos_skill.main \
  --profile my-profile \
  --accounts 123456789012 \
  --regions us-west-2
```

#### Multiple regions

```bash
python -m eos_skill.main \
  --profile my-profile \
  --accounts 123456789012 \
  --regions us-west-2 us-west-1 ap-northeast-1
```

#### Multiple accounts + multiple regions

```bash
python -m eos_skill.main \
  --profile my-profile \
  --accounts 123456789012 987654321098 \
  --regions us-west-2 us-east-1 ap-northeast-1
```

#### Specify output path

```bash
# Output to Desktop
python -m eos_skill.main \
  --profile my-profile \
  --accounts 123456789012 \
  --regions us-west-2 \
  --output ~/Desktop/eos_report.xlsx

# Output to specific directory
python -m eos_skill.main \
  --profile my-profile \
  --accounts 123456789012 \
  --regions us-west-2 us-west-1 \
  --output /tmp/reports/eos_report_prod.xlsx
```

#### Scan specific resource types only

```bash
# Only RDS and EKS
python -m eos_skill.main \
  --profile my-profile \
  --accounts 123456789012 \
  --regions us-west-2 \
  --resource-types rds eks

# Only ElastiCache
python -m eos_skill.main \
  --profile my-profile \
  --accounts 123456789012 \
  --regions us-west-2 \
  --resource-types elasticache
```

#### Using AK/SK instead of profile

```bash
python -m eos_skill.main \
  --access-key AKIAXXXXXXXX \
  --secret-key XXXXXXXXXXXXXXXX \
  --accounts 123456789012 \
  --regions us-west-2 us-west-1 \
  --output ~/Desktop/eos_report.xlsx
```

#### Cross-account with custom role name

```bash
python -m eos_skill.main \
  --profile my-profile \
  --accounts 123456789012 987654321098 \
  --regions us-west-2 \
  --role-name MyCustomCrossAccountRole
```

## All Options

| Option | Required | Description |
|--------|----------|-------------|
| `--accounts` | Yes | One or more AWS account IDs |
| `--regions` | Yes | One or more AWS regions |
| `--profile` | No | AWS CLI profile name |
| `--access-key` | No | AWS Access Key ID (requires `--secret-key`) |
| `--secret-key` | No | AWS Secret Access Key (requires `--access-key`) |
| `--resource-types` | No | `rds`, `elasticache`, `eks`, `documentdb`, `neptune`, `opensearch`, `msk`, `lambda`, `amazonmq` (default: all) |
| `--role-name` | No | Cross-account IAM role (default: `OrganizationAccountAccessRole`) |
| `--output` | No | Output file path (default: `eos_report_<timestamp>.xlsx`) |

## Report Columns

| # | Field | Description |
|---|-------|-------------|
| 1 | Account | AWS account ID |
| 2 | Region | AWS region |
| 3 | Cluster Name | Cluster/domain/replication group name |
| 4 | Instance Name | Member instance(s) or standalone resource name |
| 5 | Engine | MySQL, PostgreSQL, Redis, etc. |
| 6 | Resource Type | RDS, Aurora, ElastiCache, EKS, DocumentDB, Neptune, OpenSearch, MSK, Lambda, Amazon MQ |
| 7 | Instance Type | e.g., db.t3.medium |
| 8 | Engine Version | Current version |
| 9 | End of Support Date | Standard support end date (from endoflife.date) |
| 10 | Extended Support Date | Extended support end date |
| 11 | Target Engine Version | Recommended upgrade version |
| 12 | Upgrade Type | Major / Minor |

## EOS Data Sources

| Data | Source | Method |
|------|--------|--------|
| End of Support (EOS) Date | [endoflife.date](https://endoflife.date) API | Dynamic, real-time fetch via `https://endoflife.date/api/{product}.json` |
| Extended Support (ES) Date | [endoflife.date](https://endoflife.date) API | Dynamic, `extendedSupport` field from the same API |
| Target Engine Version | AWS APIs + `eos_skill/eos_data.py` | Dynamic latest from AWS `describe_*_versions` APIs, with static fallback |
| Upgrade Type (Major/Minor) | Computed | Engine-specific version comparison logic (see below) |

Supported endoflife.date product mappings:

| Service | endoflife.date Product |
|---------|----------------------|
| RDS MySQL | [amazon-rds-mysql](https://endoflife.date/amazon-rds-mysql) |
| RDS PostgreSQL | [amazon-rds-postgresql](https://endoflife.date/amazon-rds-postgresql) |
| RDS MariaDB | [amazon-rds-mariadb](https://endoflife.date/amazon-rds-mariadb) |
| Aurora PostgreSQL | [amazon-aurora-postgresql](https://endoflife.date/amazon-aurora-postgresql) |
| ElastiCache Redis | [amazon-elasticache-redis](https://endoflife.date/amazon-elasticache-redis) |
| EKS | [amazon-eks](https://endoflife.date/amazon-eks) |
| DocumentDB | [amazon-documentdb](https://endoflife.date/amazon-documentdb) |
| Neptune | [amazon-neptune](https://endoflife.date/amazon-neptune) |
| OpenSearch | [amazon-opensearch](https://endoflife.date/amazon-opensearch) |
| MSK | [amazon-msk](https://endoflife.date/amazon-msk) |
| Lambda | [aws-lambda](https://endoflife.date/aws-lambda) |
| ActiveMQ | [apache-activemq](https://endoflife.date/apache-activemq) |
| RabbitMQ | [rabbitmq](https://endoflife.date/rabbitmq) |

## Upgrade Type Logic (Major / Minor)

The upgrade type is determined by engine-specific version comparison rules:

| Engine | Major Version Parts | Major Example | Minor Example |
|--------|-------------------|---------------|---------------|
| **RDS MySQL** | First 2 parts (`X.Y`) | 8.0 → 8.4, 5.7 → 8.0 | 8.0.35 → 8.0.40 |
| **RDS PostgreSQL** | First part (`X`) | 13 → 16, 15 → 16 | 16.4 → 16.13 |
| **RDS MariaDB** | First 2 parts (`X.Y`) | 10.4 → 10.11, 10.6 → 10.11 | 10.11.6 → 10.11.8 |
| **Aurora MySQL** | First 2 parts (`X.Y`) | 2.x → 3.x (MySQL 5.7 → 8.0) | 3.04 → 3.07 |
| **Aurora PostgreSQL** | First part (`X`) | 13 → 16, 14 → 16 | 16.4 → 16.13 |
| **ElastiCache Redis** | First part (`X`) | 6 → 7 | 6.0 → 6.2 |
| **EKS** | First 2 parts (`1.Y`), every minor version bump is Major | 1.29 → 1.30, 1.29 → 1.35 | Patch only (e.g., platform version) |
| **DocumentDB** | First part (`X`) | 3.6 → 5.0, 4.0 → 5.0 | — |
| **Neptune** | First 2 parts (`X.Y`) | 1.2 → 1.3, 1.3 → 1.4 | 1.3.2.1 → 1.3.3.0 |
| **OpenSearch** | First part (`X`) | 1.3 → 2.11 | 2.3 → 2.11 |
| **MSK Kafka** | First part (`X`) | 2.8 → 3.6 | 3.3 → 3.6 |
| **Lambda** | Full runtime identifier (any change = Major) | python3.11 → python3.12, nodejs18 → nodejs20 | Internal patches (automatic by AWS) |
| **Amazon MQ ActiveMQ** | First 2 parts (`X.Y`) | 5.15 → 5.17 | 5.17.3 → 5.17.6 |
| **Amazon MQ RabbitMQ** | First 2 parts (`X.Y`) | 3.10 → 3.13 | 3.12.5 → 3.12.14 |
