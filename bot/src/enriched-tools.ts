/**
 * Enriched cloud tools for OpsAgent.
 * Specialized tools that wrap common AWS/cloud API calls with
 * structured output, error handling, and output parsing.
 *
 * These complement the generic run_command tool by providing:
 * - Structured JSON output parsing
 * - Built-in region/account context
 * - Output truncation and formatting
 */
import type Anthropic from '@anthropic-ai/sdk';
import { execFile } from 'child_process';
import { validateCommand, SandboxConfig } from './command-sandbox';

const CMD_TIMEOUT = 60_000;
const MAX_OUTPUT = 80 * 1024; // 80KB

// ── Tool Definitions ──────────────────────────────────────────

function tool(name: string, description: string, props: Record<string, any>, required: string[] = []): Anthropic.Tool {
  return {
    name,
    description,
    input_schema: { type: 'object' as const, properties: props, required },
  };
}

const regionProp = { type: 'string', description: '区域 (例: us-east-1, cn-hangzhou)' };
const limitProp = { type: 'number', description: '返回数量上限 (默认 50)' };
const filterProp = { type: 'string', description: '过滤条件 (JSON 格式的 AWS CLI --filters 参数)' };

export const ENRICHED_TOOLS: Anthropic.Tool[] = [
  // ── Compute ─────────────────────────────────────────────
  tool('describe_ec2', '查询 EC2 实例列表及状态。返回实例 ID、类型、状态、IP、标签。', {
    region: regionProp, filters: filterProp, instance_ids: { type: 'string', description: '逗号分隔的实例 ID' },
  }, ['region']),

  tool('describe_lambda', '查询 Lambda 函数列表。返回函数名、运行时、内存、超时。', {
    region: regionProp,
  }, ['region']),

  tool('describe_ecs_services', '查询 ECS 服务及任务状态。返回集群、服务名、运行任务数、期望任务数。', {
    region: regionProp, cluster: { type: 'string', description: 'ECS 集群名称' },
  }, ['region']),

  tool('describe_eks_clusters', '查询 EKS 集群列表及状态。返回集群名、版本、状态、端点。', {
    region: regionProp,
  }, ['region']),

  tool('describe_autoscaling', '查询 Auto Scaling 组列表。返回组名、实例数、最小/最大/期望容量。', {
    region: regionProp,
  }, ['region']),

  // ── Database ────────────────────────────────────────────
  tool('describe_rds', '查询 RDS 数据库实例列表。返回实例 ID、引擎、状态、存储、多可用区。', {
    region: regionProp,
  }, ['region']),

  tool('describe_dynamodb', '查询 DynamoDB 表列表及状态。返回表名、状态、条目数、大小。', {
    region: regionProp,
  }, ['region']),

  tool('describe_elasticache', '查询 ElastiCache 集群列表。返回集群 ID、引擎、节点类型、状态。', {
    region: regionProp,
  }, ['region']),

  // ── Network ─────────────────────────────────────────────
  tool('describe_vpcs', '查询 VPC 列表。返回 VPC ID、CIDR、是否默认、标签。', {
    region: regionProp,
  }, ['region']),

  tool('describe_security_groups', '查询安全组列表。返回组 ID、组名、入站/出站规则摘要。可过滤开放 0.0.0.0/0 的规则。', {
    region: regionProp, vpc_id: { type: 'string', description: '限定 VPC ID' },
  }, ['region']),

  tool('describe_load_balancers', '查询负载均衡器（ALB/NLB）列表。返回 ARN、DNS、状态、类型。', {
    region: regionProp,
  }, ['region']),

  tool('describe_nat_gateways', '查询 NAT Gateway 列表。返回 NAT ID、状态、公网 IP、子网。', {
    region: regionProp,
  }, ['region']),

  // ── Storage ─────────────────────────────────────────────
  tool('list_s3_buckets', '列出所有 S3 存储桶。返回桶名、创建时间。', {}),

  tool('describe_ebs_volumes', '查询 EBS 卷列表。返回卷 ID、大小、类型、状态、加密状态。', {
    region: regionProp,
  }, ['region']),

  // ── Monitoring ──────────────────────────────────────────
  tool('list_alarms', '查询 CloudWatch 告警列表。返回告警名、状态（OK/ALARM/INSUFFICIENT_DATA）、指标、阈值。', {
    region: regionProp, state: { type: 'string', description: '过滤告警状态: OK, ALARM, INSUFFICIENT_DATA' },
  }, ['region']),

  tool('get_cloudwatch_metrics', '获取 CloudWatch 指标数据点。返回时间序列数据（Average/Max/Min）。', {
    region: regionProp,
    namespace: { type: 'string', description: '指标命名空间 (例: AWS/EC2, AWS/RDS)' },
    metric_name: { type: 'string', description: '指标名称 (例: CPUUtilization)' },
    dimensions: { type: 'string', description: '维度 JSON (例: [{"Name":"InstanceId","Value":"i-123"}])' },
    period: { type: 'number', description: '统计周期（秒），默认 300' },
    hours: { type: 'number', description: '回溯小时数，默认 1' },
  }, ['region', 'namespace', 'metric_name']),

  tool('query_cloudwatch_logs', '查询 CloudWatch Logs Insights。支持自定义查询语法。', {
    region: regionProp,
    log_group: { type: 'string', description: '日志组名称' },
    query: { type: 'string', description: 'Logs Insights 查询语句' },
    hours: { type: 'number', description: '回溯小时数，默认 1' },
  }, ['region', 'log_group', 'query']),

  // ── Change Tracking ─────────────────────────────────────
  tool('lookup_cloudtrail', '查询 CloudTrail 最近事件。返回 API 调用记录（谁在何时做了什么变更）。', {
    region: regionProp,
    resource_id: { type: 'string', description: '资源 ID（限定查询范围）' },
    hours: { type: 'number', description: '回溯小时数，默认 6' },
    event_name: { type: 'string', description: '过滤特定 API 操作名（例: RunInstances）' },
  }, ['region']),

  // ── Security ────────────────────────────────────────────
  tool('check_guardduty', '查询 GuardDuty 安全发现。返回严重程度、类型、受影响资源。', {
    region: regionProp,
  }, ['region']),

  // ── Resource Scanning ───────────────────────────────────
  tool('scan_resources', '触发多云资源扫描。扫描所有配置的云账号，发现并存储资源清单到数据库。返回扫描摘要。', {
    focus: { type: 'string', description: '扫描类别: all, computing, databases, networking, storage, security' },
    accounts: { type: 'string', description: '限定账号名（逗号分隔），不填则扫描所有' },
    regions: { type: 'string', description: '限定区域（逗号分隔），不填则使用账号配置' },
  }),

  tool('query_resources', '查询已扫描的资源清单（从数据库）。支持按类型、账号、状态过滤。', {
    resource_type: { type: 'string', description: '资源类型过滤 (例: EC2, RDS, Lambda)' },
    account_name: { type: 'string', description: '账号名过滤' },
    provider: { type: 'string', description: '云平台过滤: aws, alicloud, azure, gcp' },
    status: { type: 'string', description: '状态过滤 (例: running, stopped)' },
    limit: limitProp,
  }),
];

// ── Tool Executor ─────────────────────────────────────────────

export interface EnrichedToolConfig {
  env: Record<string, string | undefined>;
  workDir: string;
  sandboxConfig?: SandboxConfig;
}

/**
 * Execute an enriched tool by running the corresponding AWS CLI command.
 */
export async function executeEnrichedTool(
  name: string,
  input: Record<string, any>,
  config: EnrichedToolConfig,
): Promise<{ content: string; is_error: boolean }> {
  const cmd = buildCommand(name, input);
  if (!cmd) {
    return { content: `Unknown enriched tool: ${name}`, is_error: true };
  }

  // For DB query tools, handle separately
  if (cmd.startsWith('__db:')) {
    return executeDbTool(name, input);
  }

  return runShellCommand(cmd, config);
}

// ── Command Builders ──────────────────────────────────────────

function buildCommand(name: string, input: Record<string, any>): string | null {
  const r = input.region || '';
  const regionFlag = r ? ` --region ${r}` : '';

  switch (name) {
    // Compute
    case 'describe_ec2': {
      let cmd = `aws ec2 describe-instances${regionFlag} --output json`;
      if (input.instance_ids) cmd += ` --instance-ids ${input.instance_ids.split(',').join(' ')}`;
      if (input.filters) cmd += ` --filters '${input.filters}'`;
      return cmd;
    }
    case 'describe_lambda':
      return `aws lambda list-functions${regionFlag} --output json`;
    case 'describe_ecs_services': {
      if (input.cluster) return `aws ecs describe-services${regionFlag} --cluster ${input.cluster} --output json`;
      return `aws ecs list-clusters${regionFlag} --output json`;
    }
    case 'describe_eks_clusters':
      return `aws eks list-clusters${regionFlag} --output json`;
    case 'describe_autoscaling':
      return `aws autoscaling describe-auto-scaling-groups${regionFlag} --output json`;

    // Database
    case 'describe_rds':
      return `aws rds describe-db-instances${regionFlag} --output json`;
    case 'describe_dynamodb':
      return `aws dynamodb list-tables${regionFlag} --output json`;
    case 'describe_elasticache':
      return `aws elasticache describe-cache-clusters${regionFlag} --output json`;

    // Network
    case 'describe_vpcs':
      return `aws ec2 describe-vpcs${regionFlag} --output json`;
    case 'describe_security_groups': {
      let cmd = `aws ec2 describe-security-groups${regionFlag} --output json`;
      if (input.vpc_id) cmd += ` --filters Name=vpc-id,Values=${input.vpc_id}`;
      return cmd;
    }
    case 'describe_load_balancers':
      return `aws elbv2 describe-load-balancers${regionFlag} --output json`;
    case 'describe_nat_gateways':
      return `aws ec2 describe-nat-gateways${regionFlag} --output json`;

    // Storage
    case 'list_s3_buckets':
      return `aws s3api list-buckets --output json`;
    case 'describe_ebs_volumes':
      return `aws ec2 describe-volumes${regionFlag} --output json`;

    // Monitoring
    case 'list_alarms': {
      let cmd = `aws cloudwatch describe-alarms${regionFlag} --output json`;
      if (input.state) cmd += ` --state-value ${input.state}`;
      return cmd;
    }
    case 'get_cloudwatch_metrics': {
      const period = input.period || 300;
      const hours = input.hours || 1;
      const now = new Date();
      const start = new Date(now.getTime() - hours * 3600 * 1000).toISOString();
      let cmd = `aws cloudwatch get-metric-statistics${regionFlag}`;
      cmd += ` --namespace "${input.namespace}" --metric-name "${input.metric_name}"`;
      cmd += ` --start-time ${start} --end-time ${now.toISOString()}`;
      cmd += ` --period ${period} --statistics Average Maximum Minimum`;
      if (input.dimensions) cmd += ` --dimensions '${input.dimensions}'`;
      cmd += ` --output json`;
      return cmd;
    }
    case 'query_cloudwatch_logs': {
      const hours = input.hours || 1;
      const now = Math.floor(Date.now() / 1000);
      const start = now - hours * 3600;
      return `aws logs start-query${regionFlag} --log-group-name "${input.log_group}" --start-time ${start} --end-time ${now} --query-string '${input.query}' --output json`;
    }

    // Change Tracking
    case 'lookup_cloudtrail': {
      const hours = input.hours || 6;
      const start = new Date(Date.now() - hours * 3600 * 1000).toISOString();
      let cmd = `aws cloudtrail lookup-events${regionFlag} --start-time ${start} --output json --max-results 50`;
      if (input.resource_id) cmd += ` --lookup-attributes AttributeKey=ResourceName,AttributeValue=${input.resource_id}`;
      if (input.event_name) cmd += ` --lookup-attributes AttributeKey=EventName,AttributeValue=${input.event_name}`;
      return cmd;
    }

    // Security
    case 'check_guardduty':
      return `aws guardduty list-detectors${regionFlag} --output json`;

    // DB-backed tools
    case 'scan_resources':
      return '__db:scan_resources';
    case 'query_resources':
      return '__db:query_resources';

    default:
      return null;
  }
}

// ── Shell Execution ───────────────────────────────────────────

function runShellCommand(cmd: string, config: EnrichedToolConfig): Promise<{ content: string; is_error: boolean }> {
  return new Promise((resolve) => {
    execFile('/bin/sh', ['-c', cmd], {
      cwd: config.workDir,
      env: config.env as NodeJS.ProcessEnv,
      timeout: CMD_TIMEOUT,
      maxBuffer: MAX_OUTPUT * 2,
    }, (error, stdout, stderr) => {
      let output = stdout || '';
      if (stderr && !output) output = stderr;
      if (output.length > MAX_OUTPUT) {
        output = output.substring(0, MAX_OUTPUT) + '\n...(output truncated)';
      }
      if (error) {
        if (error.killed) resolve({ content: `Command timed out after ${CMD_TIMEOUT / 1000}s`, is_error: true });
        else resolve({ content: output || error.message, is_error: true });
      } else {
        resolve({ content: output || '(no output)', is_error: false });
      }
    });
  });
}

// ── DB-backed Tools ───────────────────────────────────────────

async function executeDbTool(name: string, input: Record<string, any>): Promise<{ content: string; is_error: boolean }> {
  try {
    const db = require('./db');

    if (name === 'scan_resources') {
      // Placeholder: scan_resources will be implemented in resource-scanner.ts
      return { content: 'Resource scanning not yet configured. Use run_command with AWS CLI directly.', is_error: false };
    }

    if (name === 'query_resources') {
      const conditions: string[] = [];
      const params: any[] = [];
      let idx = 1;
      if (input.resource_type) { conditions.push(`resource_type = $${idx++}`); params.push(input.resource_type); }
      if (input.account_name) { conditions.push(`account_name = $${idx++}`); params.push(input.account_name); }
      if (input.provider) { conditions.push(`provider = $${idx++}`); params.push(input.provider); }
      if (input.status) { conditions.push(`status = $${idx++}`); params.push(input.status); }
      const where = conditions.length > 0 ? `WHERE ${conditions.join(' AND ')}` : '';
      const limit = Math.min(input.limit || 50, 200);
      const sql = `SELECT resource_id, resource_type, name, provider, region, status, account_name, tags FROM resources ${where} ORDER BY last_seen_at DESC LIMIT ${limit}`;
      const rows = await db.query(sql, params);
      if (rows.length === 0) return { content: 'No resources found matching the filter.', is_error: false };
      // Format as markdown table
      const header = '| Resource ID | Type | Name | Provider | Region | Status | Account |';
      const sep = '|---|---|---|---|---|---|---|';
      const lines = rows.map((r: any) =>
        `| ${r.resource_id} | ${r.resource_type} | ${r.name || '-'} | ${r.provider} | ${r.region || '-'} | ${r.status || '-'} | ${r.account_name} |`
      );
      return { content: `Found ${rows.length} resources:\n\n${header}\n${sep}\n${lines.join('\n')}`, is_error: false };
    }

    return { content: `Unknown DB tool: ${name}`, is_error: true };
  } catch (err: any) {
    return { content: `DB query failed: ${err.message}`, is_error: true };
  }
}
