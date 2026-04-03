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
 * Designed for EKS / Grafana Cloud workloads with multi-signal analysis:
 *   - Grafana Cloud: Loki (logs) + Tempo (traces) + Mimir (metrics)
 *   - Kubernetes: Pod/Node resource state (kubectl)
 *   - GitHub: Application source code
 */
export function buildRcaPrompt(input: RcaInput): string {
  return `## 根因分析 (RCA) 任务

### Issue #${input.issueId}
- **标题**: ${input.title}
- **资源**: ${input.resource_id} (${input.resource_type || 'EKS Pod'})
- **严重程度**: ${input.severity}
${input.account_name ? `- **账号**: ${input.account_name}` : ''}
${input.description ? `- **描述**: ${input.description}` : ''}
${input.metric_data ? `- **告警原始数据**: \`\`\`json\n${JSON.stringify(input.metric_data, null, 2)}\n\`\`\`` : ''}

---

### 多信号综合分析协议

以下各节已由 OpsAgent 自动采集，请**逐节分析**，最终给出综合根因判断。

#### 分析维度与权重

| 信号 | 分析重点 |
|------|---------|
| **Mimir 指标** | 错误率趋势、连接池饱和、内存增长曲线 — 判断故障是突发还是渐进 |
| **Tempo Traces** | 错误 Span 链路、调用耗时分布 — 定位哪个调用点失败 |
| **Loki 日志** | 具体错误消息、异常堆栈、时序 — 确认根因发生时刻 |
| **Pod/Node 资源** | CPU/MEM 实际用量 vs limits — 判断是否资源压力引发 |
| **源码分析** | 连接/资源泄漏路径、硬编码限制、K8s 配置 — 代码层根因 |

---

### 综合分析要求

1. **交叉验证**：指标趋势 → Trace 链路 → 日志错误消息 → 源码逻辑，必须时序对齐
2. **代码层证据**：如日志显示 \`DatabaseConnectionError\`，需在源码中找到连接池配置和异常处理路径
3. **可操作建议**：每条建议需精确到具体文件/配置项（如 \`k8s/configmap.yaml\` 中的 \`DB_POOL_MAX\`）
4. **置信度校准**：
   - 0.9+: 四个信号交叉验证一致，源码有直接证据
   - 0.7–0.9: 三个信号对齐 + 源码支持
   - 0.5–0.7: 两个信号 + 推断
   - <0.5: 信号不足，需要更多调查

---

### 输出格式

请以如下 JSON 格式输出分析结果（直接输出 JSON，不要额外解释）:
\`\`\`json
{
  "root_cause": "一句话根因描述，要包含具体代码/配置位置",
  "confidence": 0.85,
  "contributing_factors": [
    "因素1（引用具体指标/日志/代码行）",
    "因素2",
    "因素3"
  ],
  "recommendations": [
    "建议1 — 精确到文件和配置项",
    "建议2",
    "建议3"
  ],
  "fix_plan": {
    "steps": ["步骤1", "步骤2", "步骤3"],
    "estimated_time": "15分钟",
    "requires_downtime": false
  },
  "fix_risk_level": "low|medium|high|critical",
  "evidence": {
    "metrics": "指标异常摘要（峰值、趋势）",
    "traces": "错误 Trace 摘要（traceId、失败 Span）",
    "logs": "关键错误日志摘要（时间、消息）",
    "kubectl": "Pod/Node 资源摘要",
    "source_code": "源码关键发现（文件名、行为描述）"
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
