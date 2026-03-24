/**
 * Health Patrol Engine for OpsAgent.
 * Creates and manages health issues, builds patrol prompts,
 * and provides issue lifecycle management.
 */
import * as crypto from 'crypto';
import type { Issue } from './db';

// ── Issue Fingerprint ─────────────────────────────────────────

/**
 * Generate a 16-char hex fingerprint for deduplication.
 */
export function issueFingerprint(resourceId: string, title: string, severity: string): string {
  const input = `${resourceId}:${title}:${severity}`;
  return crypto.createHash('md5').update(input).digest('hex').substring(0, 16);
}

// ── Issue CRUD (in-memory helpers) ────────────────────────────

export interface CreateIssueInput {
  resource_id: string;
  resource_type?: string;
  severity: string;
  source: string;
  title: string;
  description?: string;
  account_name?: string;
  tenant_id?: string;
  metric_data?: Record<string, any>;
  related_changes?: any[];
}

/**
 * Create a new Issue object (in-memory, not persisted).
 * Use db.query() to INSERT into PostgreSQL.
 */
export function createIssue(input: CreateIssueInput): Issue {
  const now = new Date().toISOString();
  return {
    id: 0, // Will be assigned by DB
    resource_id: input.resource_id,
    resource_type: input.resource_type || null,
    severity: input.severity,
    status: 'open',
    source: input.source,
    title: input.title,
    description: input.description || null,
    fingerprint: issueFingerprint(input.resource_id, input.title, input.severity),
    occurrence_count: 1,
    metric_data: input.metric_data || {},
    related_changes: input.related_changes || [],
    tenant_id: input.tenant_id || null,
    account_name: input.account_name || null,
    created_at: now,
    updated_at: now,
    resolved_at: null,
  };
}

/**
 * Increment occurrence_count for a duplicate issue.
 */
export function deduplicateIssue(existing: Issue): Issue {
  return {
    ...existing,
    occurrence_count: existing.occurrence_count + 1,
    updated_at: new Date().toISOString(),
  };
}

/**
 * Mark an issue as resolved.
 */
export function resolveIssue(issue: Issue): Issue {
  return {
    ...issue,
    status: 'resolved',
    resolved_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
  };
}

/**
 * Filter issues from a list.
 */
export function listIssues(
  issues: Issue[],
  filter?: { status?: string; severity?: string; tenant_id?: string },
): Issue[] {
  let result = [...issues];
  if (filter?.status) result = result.filter(i => i.status === filter.status);
  if (filter?.severity) result = result.filter(i => i.severity === filter.severity);
  if (filter?.tenant_id) result = result.filter(i => i.tenant_id === filter.tenant_id);
  return result;
}

/**
 * Find an issue by ID.
 */
export function getIssue(issues: Issue[], id: number): Issue | null {
  return issues.find(i => i.id === id) || null;
}

// ── Patrol Prompt Builder ─────────────────────────────────────

export interface PatrolConfig {
  accountName: string;
  scope: string; // 'all' | 'computing' | 'databases' | 'networking' | 'security'
  regions: string[];
  deep?: boolean;
}

/**
 * Build the patrol prompt for the Claude agent.
 * This prompt instructs Claude to check health using enriched tools
 * and report findings.
 */
export function buildPatrolPrompt(config: PatrolConfig): string {
  const regions = config.regions.length > 0 ? config.regions.join(', ') : '所有配置的区域';
  const scope = config.scope || 'all';

  const parts: string[] = [
    `## 健康巡检任务`,
    ``,
    `对账号 **${config.accountName}** 执行健康巡检。`,
    `区域范围: ${regions}`,
    `检查范围: ${scope}`,
    ``,
    `### 巡检步骤`,
    ``,
    `1. **告警检查**: 使用 \`list_alarms\` 查看 CloudWatch 告警状态`,
    `   - 重点关注 ALARM 状态的告警`,
    `   - 如果所有告警 OK，记录"无告警"`,
    ``,
    `2. **资源健康检查** (根据 scope):`,
  ];

  if (scope === 'all' || scope === 'computing') {
    parts.push(`   - EC2: 使用 \`describe_ec2\` 检查实例状态`);
    parts.push(`   - EKS: 使用 \`describe_eks_clusters\` 检查集群版本和状态`);
    parts.push(`   - Lambda: 使用 \`describe_lambda\` 检查函数错误率`);
  }
  if (scope === 'all' || scope === 'databases') {
    parts.push(`   - RDS: 使用 \`describe_rds\` 检查数据库状态和存储`);
    parts.push(`   - ElastiCache: 使用 \`describe_elasticache\` 检查缓存状态`);
  }
  if (scope === 'all' || scope === 'networking') {
    parts.push(`   - NAT Gateway: 使用 \`describe_nat_gateways\` 检查状态`);
    parts.push(`   - Load Balancer: 使用 \`describe_load_balancers\` 检查健康目标`);
  }

  if (scope === 'all' || scope === 'security') {
    parts.push(``, `3. **安全检查**:`);
    parts.push(`   - GuardDuty: 使用 \`check_guardduty\` 检查安全威胁`);
    parts.push(`   - 安全组: 使用 \`describe_security_groups\` 检查开放 0.0.0.0/0 的规则`);
    parts.push(`   - CloudTrail: 使用 \`lookup_cloudtrail\` 查看最近变更`);
  }

  if (config.deep) {
    parts.push(``, `4. **深入分析** (deep 模式):`);
    parts.push(`   - 对每个异常资源，使用 \`get_cloudwatch_metrics\` 获取 CPU/内存/网络指标`);
    parts.push(`   - 使用 \`query_cloudwatch_logs\` 查看错误日志`);
  }

  parts.push(``, `### 输出格式`);
  parts.push(``);
  parts.push(`对每个发现的问题，按以下格式报告:`);
  parts.push(``);
  parts.push(`| 严重程度 | 资源 | 问题 | 详情 |`);
  parts.push(`|----------|------|------|------|`);
  parts.push(`| CRITICAL/HIGH/MEDIUM/LOW | 资源ID | 标题 | 描述 |`);
  parts.push(``);
  parts.push(`严重程度分级:`);
  parts.push(`- **CRITICAL**: 服务不可达、实例宕机、数据库连接失败`);
  parts.push(`- **HIGH**: 性能退化（CPU >90%、错误率 >5%）`);
  parts.push(`- **MEDIUM**: 容量预警（磁盘 >80%、连接 >70%）`);
  parts.push(`- **LOW**: 治理问题（安全配置、过期凭证、合规）`);
  parts.push(``);
  parts.push(`如果一切正常，输出: "巡检完成，未发现异常。"`);

  return parts.join('\n');
}

// ── DB Persistence Helpers ────────────────────────────────────

/**
 * Save an issue to PostgreSQL. Returns the created/updated issue.
 * Uses fingerprint for deduplication: if same fingerprint exists,
 * increments occurrence_count instead of creating a new row.
 */
export async function saveIssueToDB(issue: Issue): Promise<Issue> {
  const { query, queryOne } = require('./db') as typeof import('./db');

  // Check for duplicate by fingerprint
  if (issue.fingerprint) {
    const existing = await queryOne(
      `SELECT * FROM issues WHERE fingerprint = $1 AND status != 'resolved'`,
      [issue.fingerprint],
    ) as Issue | null;
    if (existing) {
      await query(
        `UPDATE issues SET occurrence_count = occurrence_count + 1, updated_at = NOW() WHERE id = $1`,
        [existing.id],
      );
      return { ...existing, occurrence_count: existing.occurrence_count + 1 };
    }
  }

  // Insert new issue
  const row = await queryOne(
    `INSERT INTO issues (resource_id, resource_type, severity, status, source, title, description,
      fingerprint, occurrence_count, metric_data, related_changes, tenant_id, account_name)
     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
     RETURNING *`,
    [issue.resource_id, issue.resource_type, issue.severity, issue.status, issue.source,
     issue.title, issue.description, issue.fingerprint, issue.occurrence_count,
     JSON.stringify(issue.metric_data), JSON.stringify(issue.related_changes),
     issue.tenant_id, issue.account_name],
  ) as Issue | null;
  return row || issue;
}
