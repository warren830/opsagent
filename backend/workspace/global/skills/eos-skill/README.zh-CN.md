[English](README.md) | [中文](README.zh-CN.md)

# EOS Skill - AWS 停止支持资源扫描器

扫描 AWS 资源的停止支持（EOS）状态，生成 Excel 升级报告。

## 功能

- 扫描 RDS 实例和 Aurora 集群（MySQL、PostgreSQL、MariaDB）
- 扫描 ElastiCache 集群（Redis、Memcached）
- 扫描 EKS Kubernetes 集群
- 扫描 DocumentDB 集群（MongoDB 兼容）
- 扫描 Neptune 图数据库集群
- 扫描 OpenSearch / Elasticsearch 域
- 扫描 MSK Kafka 集群
- 扫描 Lambda 函数（运行时弃用检测）
- 扫描 Amazon MQ 代理（ActiveMQ、RabbitMQ）
- 颜色编码 Excel 报告（红色=已过期，黄色=即将过期，绿色=安全）
- 双语表头（英文 + 中文）
- 通过 IAM 角色跨账号扫描

## 安装

### 通过 npx 安装

```bash
npx skills add https://github.com/koljahuang/eos-skill.git
```

出现提示时，选择 agent 目录：

```
── Additional agents ──────────────────────────────
❯ ● Kiro CLI (.kiro/skills)
```

Skill 将安装到 `.kiro/skills/eos-skill/` 目录。

### 手动安装

```bash
# 克隆到 .kiro/skills 目录
mkdir -p .kiro/skills
git clone https://github.com/koljahuang/eos-skill.git .kiro/skills/eos-skill

# 安装依赖
pip install boto3 openpyxl
```

### 在 Kiro 中使用

1. 打开 Kiro IDE
2. Skill 会自动从 `.kiro/skills/eos-skill/` 加载
3. 在对话中输入 `eos report` 开始扫描

## 使用方式

### 方式一：Claude Code / Kiro Skill 调用

在对话中输入：

```
eos report
```

Claude 会询问以下信息：
1. AWS 认证方式（profile 或 AK/SK）
2. 账号 ID
3. 区域
4. 资源类型

### 方式二：命令行直接运行

#### 基础用法 - 单区域

```bash
python -m eos_skill.main \
  --profile my-profile \
  --accounts 123456789012 \
  --regions us-west-2
```

#### 多区域扫描

```bash
python -m eos_skill.main \
  --profile my-profile \
  --accounts 123456789012 \
  --regions us-west-2 us-west-1 ap-northeast-1
```

#### 多账号 + 多区域

```bash
python -m eos_skill.main \
  --profile my-profile \
  --accounts 123456789012 987654321098 \
  --regions us-west-2 us-east-1 ap-northeast-1
```

#### 指定输出路径

```bash
# 输出到桌面
python -m eos_skill.main \
  --profile my-profile \
  --accounts 123456789012 \
  --regions us-west-2 \
  --output ~/Desktop/eos_report.xlsx

# 输出到指定目录
python -m eos_skill.main \
  --profile my-profile \
  --accounts 123456789012 \
  --regions us-west-2 us-west-1 \
  --output /tmp/reports/eos_report_prod.xlsx
```

#### 只扫描特定资源类型

```bash
# 仅扫描 RDS 和 EKS
python -m eos_skill.main \
  --profile my-profile \
  --accounts 123456789012 \
  --regions us-west-2 \
  --resource-types rds eks

# 仅扫描 ElastiCache
python -m eos_skill.main \
  --profile my-profile \
  --accounts 123456789012 \
  --regions us-west-2 \
  --resource-types elasticache
```

#### 使用 AK/SK 认证

```bash
python -m eos_skill.main \
  --access-key AKIAXXXXXXXX \
  --secret-key XXXXXXXXXXXXXXXX \
  --accounts 123456789012 \
  --regions us-west-2 us-west-1 \
  --output ~/Desktop/eos_report.xlsx
```

#### 跨账号自定义角色名

```bash
python -m eos_skill.main \
  --profile my-profile \
  --accounts 123456789012 987654321098 \
  --regions us-west-2 \
  --role-name MyCustomCrossAccountRole
```

## 全部参数

| 参数 | 必填 | 说明 |
|------|------|------|
| `--accounts` | 是 | AWS 账号 ID（支持多个，空格分隔） |
| `--regions` | 是 | AWS 区域（支持多个，空格分隔） |
| `--profile` | 否 | AWS CLI 配置名 |
| `--access-key` | 否 | AWS Access Key ID（需配合 `--secret-key`） |
| `--secret-key` | 否 | AWS Secret Access Key（需配合 `--access-key`） |
| `--resource-types` | 否 | `rds`、`elasticache`、`eks`、`documentdb`、`neptune`、`opensearch`、`msk`、`lambda`、`amazonmq`（默认扫描全部） |
| `--role-name` | 否 | 跨账号 IAM 角色名（默认: `OrganizationAccountAccessRole`） |
| `--output` | 否 | 输出文件路径（默认: `eos_report_<timestamp>.xlsx`） |

## 报告字段

| # | 字段名称 | 说明 |
|---|---------|------|
| 1 | Account (账号) | 资源所属 AWS 账号 |
| 2 | Region (区域) | 资源物理区域 |
| 3 | Cluster Name (集群名称) | 集群/域/复制组名称 |
| 4 | Instance Name (实例名称) | 成员实例或独立资源名称 |
| 5 | Engine (引擎) | MySQL、PostgreSQL、Redis 等 |
| 6 | Resource Type (资源类型) | RDS、Aurora、ElastiCache、EKS、DocumentDB、Neptune、OpenSearch、MSK、Lambda、Amazon MQ |
| 7 | Instance Type (实例类型) | 当前规格（如 db.t3.medium） |
| 8 | Engine Version (引擎版本) | 当前运行的版本号 |
| 9 | End of Support Date (停止支持日期) | 标准支持截止日期（来自 endoflife.date） |
| 10 | Extended Support Date (延长支持日期) | 延长支持截止日期 |
| 11 | Target Engine Version (目标版本号) | 建议升级到的具体版本 |
| 12 | Upgrade Type (更新类型) | Major（大版本）/ Minor（小版本） |

## EOS 数据来源

| 数据 | 来源 | 获取方式 |
|------|------|---------|
| 停止支持日期 (EOS) | [endoflife.date](https://endoflife.date) API | 实时动态获取，`https://endoflife.date/api/{product}.json` |
| 延长支持日期 (ES) | [endoflife.date](https://endoflife.date) API | 同一 API 的 `extendedSupport` 字段 |
| 目标引擎版本 | AWS API + `eos_skill/eos_data.py` | AWS `describe_*_versions` API 动态获取最新版本，静态数据兜底 |
| 更新类型 (Major/Minor) | 计算得出 | 按引擎的版本比较规则判断（见下表） |

支持的 endoflife.date 产品映射：

| 服务 | endoflife.date 产品 |
|------|-------------------|
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

## 更新类型判断逻辑 (Major / Minor)

更新类型根据各引擎的版本比较规则判断：

| 引擎 | 大版本判断依据 | Major 示例 | Minor 示例 |
|------|-------------|-----------|-----------|
| **RDS MySQL** | 前两位版本号 (`X.Y`) | 8.0 → 8.4, 5.7 → 8.0 | 8.0.35 → 8.0.40 |
| **RDS PostgreSQL** | 第一位版本号 (`X`) | 13 → 16, 15 → 16 | 16.4 → 16.13 |
| **RDS MariaDB** | 前两位版本号 (`X.Y`) | 10.4 → 10.11, 10.6 → 10.11 | 10.11.6 → 10.11.8 |
| **Aurora MySQL** | 前两位版本号 (`X.Y`) | 2.x → 3.x (MySQL 5.7 → 8.0) | 3.04 → 3.07 |
| **Aurora PostgreSQL** | 第一位版本号 (`X`) | 13 → 16, 14 → 16 | 16.4 → 16.13 |
| **ElastiCache Redis** | 第一位版本号 (`X`) | 6 → 7 | 6.0 → 6.2 |
| **EKS** | 前两位版本号 (`1.Y`)，每个小版本升级均视为 Major | 1.29 → 1.30, 1.29 → 1.35 | 补丁版本（如平台版本号） |
| **DocumentDB** | 第一位版本号 (`X`) | 3.6 → 5.0, 4.0 → 5.0 | — |
| **Neptune** | 前两位版本号 (`X.Y`) | 1.2 → 1.3, 1.3 → 1.4 | 1.3.2.1 → 1.3.3.0 |
| **OpenSearch** | 第一位版本号 (`X`) | 1.3 → 2.11 | 2.3 → 2.11 |
| **MSK Kafka** | 第一位版本号 (`X`) | 2.8 → 3.6 | 3.3 → 3.6 |
| **Lambda** | 运行时标识符全称（任何变化均为 Major） | python3.11 → python3.12, nodejs18 → nodejs20 | 运行时内部 Patch（AWS 自动更新） |
| **Amazon MQ ActiveMQ** | 前两位版本号 (`X.Y`) | 5.15 → 5.17 | 5.17.3 → 5.17.6 |
| **Amazon MQ RabbitMQ** | 前两位版本号 (`X.Y`) | 3.10 → 3.13 | 3.12.5 → 3.12.14 |
