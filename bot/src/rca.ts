/**
 * RCA (Root Cause Analysis) Engine for OpsAgent.
 * Builds investigation prompts, creates RCA results,
 * and manages the analysis lifecycle.
 */
import type { RcaResult } from './db';

// ── Types ─────────────────────────────────────────────────────

export interface RcaInput {
  issueId: number;
  title: string;
  resource_id: string;
  resource_type?: string;
  severity: string;
  description?: string;
  account_name?: string;
  regions: string[];
  metric_data?: Record<string, any>;
  related_changes?: any[];
}

export interface CreateRcaInput {
  issue_id: number;
  root_cause: string;
  confidence: number;
  contributing_factors: string[];
  recommendations: string[];
  fix_plan: Record<string, any>;
  fix_risk_level: string;
  evidence: Record<string, any>;
  model_id: string;
  duration_ms: number;
}

// ── Confidence Parsing ────────────────────────────────────────

/**
 * Parse and normalize confidence score to [0.0, 1.0] range.
 */
export function parseConfidence(value: number): number {
  if (value === undefined || value === null || isNaN(value)) return 0.5;
  const clamped = Math.max(0, Math.min(1, value));
  return Math.round(clamped * 100) / 100;
}

// ── RCA Result Creation ───────────────────────────────────────

/**
 * Create an RCA result object (in-memory).
 */
export function createRcaResult(input: CreateRcaInput): RcaResult {
  return {
    id: 0,
    issue_id: input.issue_id,
    root_cause: input.root_cause,
    confidence: parseConfidence(input.confidence),
    contributing_factors: input.contributing_factors,
    recommendations: input.recommendations,
    fix_plan: input.fix_plan,
    fix_risk_level: input.fix_risk_level,
    evidence: input.evidence,
    model_id: input.model_id,
    duration_ms: input.duration_ms,
    created_at: new Date().toISOString(),
  };
}

// ── RCA Prompt Builder ────────────────────────────────────────

/**
 * Build the RCA investigation prompt for Claude.
 * Follows an 8-step protocol inspired by AgenticOps-Chat.
 */
export function buildRcaPrompt(input: RcaInput): string {
  const regions = input.regions.length > 0 ? input.regions.join(', ') : '未指定';

  return `## 根因分析 (RCA) 任务

### Issue #${input.issueId}
- **标题**: ${input.title}
- **资源**: ${input.resource_id} (${input.resource_type || '未知类型'})
- **严重程度**: ${input.severity}
- **账号**: ${input.account_name || '未指定'}
- **区域**: ${regions}
${input.description ? `- **描述**: ${input.description}` : ''}
${input.metric_data ? `- **指标数据**: ${JSON.stringify(input.metric_data)}` : ''}

### 调查协议（按顺序执行）

**步骤 1: 变更检测**
使用 \`lookup_cloudtrail\` 查看最近 6 小时内的 API 变更。
80% 的问题由变更引起。查找与资源 ${input.resource_id} 相关的修改操作。

**步骤 2: 指标分析**
使用 \`get_cloudwatch_metrics\` 获取关键指标:
- CPU、内存、网络、磁盘 I/O
- 错误率、延迟 P99
- 连接数、队列深度

**步骤 3: 日志检查**
使用 \`query_cloudwatch_logs\` 搜索错误日志:
- 搜索 "ERROR"、"Exception"、"OOM"、"timeout" 等关键词
- 关注事件发生前后 30 分钟的日志

**步骤 4: 网络检查**
如果问题涉及连接或可达性:
- 使用 \`describe_security_groups\` 检查安全组规则
- 使用 \`describe_nat_gateways\` 检查 NAT 状态
- 使用 \`describe_load_balancers\` 检查目标健康

**步骤 5: 知识库搜索**
使用 \`lookup_skill\` 和 \`search_files\` 查找相关 SOP 和历史案例。

**步骤 6: 综合分析**
基于收集的证据，确定:

1. **根因 (root_cause)**: 一句话描述根本原因
2. **置信度 (confidence)**: 0.0-1.0
   - 0.9+: 有直接因果证据（如 CloudTrail 显示变更导致故障）
   - 0.7-0.9: 强相关性（指标异常 + 时间吻合）
   - 0.5-0.7: 合理推测（部分证据支持）
   - <0.5: 需要更多调查
3. **贡献因素**: 列出所有相关的贡献因素
4. **修复建议**: 具体可执行的修复步骤
5. **修复风险等级**:
   - low: 配置调整，不影响服务
   - medium: 需要重启或短暂中断
   - high: 可能影响其他服务
   - critical: 需要维护窗口

### 输出格式

请以 JSON 格式输出分析结果:
\`\`\`json
{
  "root_cause": "根因描述",
  "confidence": 0.85,
  "contributing_factors": ["因素1", "因素2"],
  "recommendations": ["建议1", "建议2", "建议3"],
  "fix_plan": {
    "steps": ["步骤1", "步骤2"],
    "estimated_time": "15分钟",
    "requires_downtime": false
  },
  "fix_risk_level": "medium",
  "evidence": {
    "cloudtrail": "相关变更摘要",
    "metrics": "异常指标摘要",
    "logs": "错误日志摘要"
  }
}
\`\`\``;
}

// ── DB Persistence ────────────────────────────────────────────

/**
 * Save RCA result to PostgreSQL and update issue status.
 */
export async function saveRcaResultToDB(result: RcaResult): Promise<RcaResult> {
  const { queryOne, query } = require('./db') as typeof import('./db');

  const row = await queryOne(
    `INSERT INTO rca_results (issue_id, root_cause, confidence, contributing_factors,
      recommendations, fix_plan, fix_risk_level, evidence, model_id, duration_ms)
     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
     RETURNING *`,
    [result.issue_id, result.root_cause, result.confidence,
     JSON.stringify(result.contributing_factors), JSON.stringify(result.recommendations),
     JSON.stringify(result.fix_plan), result.fix_risk_level,
     JSON.stringify(result.evidence), result.model_id, result.duration_ms],
  ) as RcaResult | null;

  // Update issue status
  await query(
    `UPDATE issues SET status = 'root_cause_identified', updated_at = NOW() WHERE id = $1`,
    [result.issue_id],
  );

  return row || result;
}
