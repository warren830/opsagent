use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use sqlx::PgPool;
use tokio::sync::{Mutex, broadcast};
use uuid::Uuid;

use crate::config::AppConfig;
use crate::models::issue::Issue;
use crate::services::agent::{Agent, AgentEvent, AgentSessionConfig};
use crate::services::claude::StreamChunk;

/// In-memory registry of currently-running RCA analyses.
/// Key = issue_id, Value = broadcast sender for streaming chunks.
pub struct RcaRegistry {
    active: Mutex<HashMap<Uuid, broadcast::Sender<StreamChunk>>>,
}

impl RcaRegistry {
    pub fn new() -> Self {
        Self {
            active: Mutex::new(HashMap::new()),
        }
    }

    /// Subscribe to an existing RCA stream, if one is running for this issue.
    pub async fn subscribe(&self, issue_id: Uuid) -> Option<broadcast::Receiver<StreamChunk>> {
        let map = self.active.lock().await;
        map.get(&issue_id).map(|tx| tx.subscribe())
    }

    /// Check whether an RCA is currently running for this issue.
    pub async fn is_running(&self, issue_id: Uuid) -> bool {
        self.active.lock().await.contains_key(&issue_id)
    }

    /// Register a new RCA. Returns (sender, receiver) pair.
    async fn register(&self, issue_id: Uuid) -> (broadcast::Sender<StreamChunk>, broadcast::Receiver<StreamChunk>) {
        let (tx, rx) = broadcast::channel(256);
        self.active.lock().await.insert(issue_id, tx.clone());
        (tx, rx)
    }

    /// Remove a completed RCA from the registry.
    async fn remove(&self, issue_id: Uuid) {
        self.active.lock().await.remove(&issue_id);
    }
}

/// Build the RCA prompt with issue context and available telemetry endpoints.
fn build_rca_prompt(issue: &Issue) -> String {
    let description = issue.description.as_deref().unwrap_or("N/A");
    let meta = issue
        .rca_result
        .as_ref()
        .map(|v| serde_json::to_string_pretty(v).unwrap_or_default())
        .unwrap_or_else(|| "N/A".to_string());

    format!(
        r#"你是一名 SRE 根因分析助理。请针对下述基础设施事件进行一次完整的根因分析。

## 事件信息
- **标题**: {title}
- **严重程度**: {severity}
- **来源**: {source}
- **类型**: {issue_type}
- **描述**: {description}
- **告警元数据**:
```json
{meta}
```
- **创建时间**: {created_at}

## 集群上下文
- 命名空间 `rca` 包含三个服务: `rca-demo`（订单服务，告警主体）、`payment-service`、`inventory-service`（金丝雀部署，由 Argo Rollouts 管理）
- 监控栈: Mimir (PromQL 指标) / Loki (LogQL 日志) / Tempo (TraceQL 链路)
- 知识库: 通过 `mcp__graphrag__rag_tool` 查询, `context_id="openops"` 包含 `rca-demo-ops-manual.pdf`，其中有 rca-demo 服务架构、金丝雀异常处理章节

## 工作规则（非常重要）
1. **输出语言全部使用中文**。包括你的思考（thinking）、过渡说明、段落标题、工具调用前的一句话解释 —— **禁止使用英文**。只有工具命令本身（如 `kubectl get pod`）和技术标识符（如 `OOMKilled`、`BUGGY=true`、服务名）可以保留英文。
2. **每次调用工具前，先用一句中文说明"为什么要调这个工具"**。例如："接下来查询 Mimir 确认错误率来源"而不是 "Let me check Mimir metrics"。
3. **主动查证，拒绝猜测**。至少调用：1 次 kubectl（查 pod/rollout 状态），2-3 次 curl PromQL/LogQL（查 Mimir + Loki），1 次 graphrag rag_tool（查 runbook）。
4. **交叉引用证据**: 报告中每个结论都要附 `[证据: <kubectl输出片段>]` 或 `[Runbook: <章节名>]`。

## 报告格式（中文 markdown）

### 概要
1-2 句话给出根因结论。

### 时间线
按时间顺序列出关键事件，每条带证据。

### 根因分析
- **直接原因**:
- **深层原因**:
- **触发条件**:
- **Runbook 引用**: 标注 `[Runbook: <章节标题>]`

### 影响范围
- 受影响的服务 / 组件
- 用户影响程度

### 修复建议
#### 立即行动
- 紧急缓解（如涉及 Argo Rollout 回滚，明确写出 `kubectl argo rollouts abort inventory-service -n rca` 这类命令）
- **明确给出一句"建议操作 X 的原因"**，让下一步执行人看懂为什么要这么做

#### 长期改进
- 架构/流程层面的改进

严格遵守: 所有 narrative 文本、所有段落标题、所有过渡用语全部用中文。"#,
        title = issue.title,
        severity = issue.severity,
        source = issue.source,
        issue_type = issue.issue_type,
        description = description,
        meta = meta,
        created_at = issue.created_at,
    )
}

/// Load default LLM provider env vars (so RCA uses the same Bedrock/gateway
/// credentials as interactive chat). Falls back to Bedrock via pod IRSA if no
/// provider is configured.
async fn load_default_provider_envs(pool: &PgPool) -> Vec<(String, String)> {
    let row: Option<(String, serde_json::Value)> = sqlx::query_as(
        "SELECT provider_type, config FROM providers WHERE is_default = TRUE ORDER BY updated_at DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    let mut env = Vec::new();
    match row {
        Some((pt, config)) if pt == "bedrock" => {
            env.push(("CLAUDE_CODE_USE_BEDROCK".to_string(), "1".to_string()));
            if let Some(r) = config.get("region").and_then(|v| v.as_str()) {
                env.push(("AWS_REGION".to_string(), r.to_string()));
            }
        }
        Some((pt, config)) if pt == "gateway" => {
            if let Some(u) = config.get("base_url").and_then(|v| v.as_str()) {
                let base = u.trim_end_matches('/').trim_end_matches("/v1").to_string();
                env.push(("ANTHROPIC_BASE_URL".to_string(), base));
            }
            if let Some(k) = config.get("api_key").and_then(|v| v.as_str()) {
                env.push(("ANTHROPIC_API_KEY".to_string(), k.to_string()));
            }
        }
        _ => {
            // Fallback: assume Bedrock via IRSA
            env.push(("CLAUDE_CODE_USE_BEDROCK".to_string(), "1".to_string()));
        }
    }
    env
}

/// Execute RCA analysis for an issue using Claude CLI.
/// Publishes StreamChunks to the broadcast channel and persists the final result to DB.
pub async fn run_rca(pool: PgPool, config: Arc<AppConfig>, registry: Arc<RcaRegistry>, issue: Issue) {
    let issue_id = issue.id;

    // Mark issue as investigating
    let _ = sqlx::query(
        "UPDATE issues SET rca_started_at = NOW(), status = 'investigating', updated_at = NOW() WHERE id = $1",
    )
    .bind(issue_id)
    .execute(&pool)
    .await;

    // Register broadcast channel
    let (tx, _rx) = registry.register(issue_id).await;

    let prompt = build_rca_prompt(&issue);
    let provider_envs = load_default_provider_envs(&pool).await;

    let agent = crate::services::agent::claude::ClaudeAgent {
        bin_path: config.claude_bin.clone(),
        work_dir: PathBuf::from(&config.claude_work_dir),
        timeout: Duration::from_millis(config.claude_timeout_ms),
    };

    let agent_config = AgentSessionConfig {
        session_id: None,
        message: prompt,
        system_prompt: None,
        model: config.claude_model.clone(),
        max_turns: 10,
        permission_mode: super::claude::AgentPermission::Bypass.cli_flag().to_string(),
        allowed_tools: Vec::new(),
        disallowed_tools: Vec::new(),
        env_vars: provider_envs,
        mcp_config_path: None,
        images: Vec::new(),
    };

    let mut full_text = String::new();

    match agent.run(agent_config) {
        Ok(mut rx) => {
            while let Some(event) = rx.recv().await {
                // Accumulate text content
                match &event {
                    AgentEvent::Text { content } => {
                        full_text.push_str(content);
                    }
                    AgentEvent::Done { content, .. } => {
                        if !content.is_empty() && full_text.is_empty() {
                            full_text = content.clone();
                        }
                    }
                    _ => {}
                }

                // Broadcast to subscribers as StreamChunk (ignore error = no subscribers)
                let chunk = StreamChunk::from_agent_event(&event);
                let _ = tx.send(chunk);
            }

            // Persist result to DB
            let rca_json = serde_json::json!({ "analysis": full_text });
            let _ = sqlx::query(
                r#"UPDATE issues SET
                   rca_result = $2,
                   rca_completed_at = NOW(),
                   status = 'rca_done',
                   updated_at = NOW()
                   WHERE id = $1"#,
            )
            .bind(issue_id)
            .bind(&rca_json)
            .execute(&pool)
            .await;

            tracing::info!("RCA completed for issue {} ({} chars)", issue_id, full_text.len());
        }
        Err(e) => {
            let error_msg = format!("Failed to spawn Claude CLI for RCA: {}", e);
            tracing::error!("{}", error_msg);

            let _ = tx.send(StreamChunk::Error {
                message: error_msg.clone(),
            });

            // Mark as failed
            let rca_json = serde_json::json!({ "error": error_msg });
            let _ = sqlx::query(
                r#"UPDATE issues SET
                   rca_result = $2,
                   rca_completed_at = NOW(),
                   status = 'open',
                   updated_at = NOW()
                   WHERE id = $1"#,
            )
            .bind(issue_id)
            .bind(&rca_json)
            .execute(&pool)
            .await;
        }
    }

    // Clean up registry
    registry.remove(issue_id).await;
}
