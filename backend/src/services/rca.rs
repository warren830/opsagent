use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use sqlx::PgPool;
use tokio::sync::{Mutex, broadcast};
use uuid::Uuid;

use crate::config::AppConfig;
use crate::models::issue::Issue;
use crate::services::claude::{ClaudeService, StreamChunk};

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
        r#"You are Ops RCA (Root Cause Analysis) Analyst. Perform a thorough root cause analysis for the following infrastructure issue.

## Issue Details
- **Title**: {title}
- **Severity**: {severity}
- **Source**: {source} (alert provider)
- **Type**: {issue_type}
- **Description**: {description}
- **Alert Metadata**:
```json
{meta}
```
- **Created**: {created_at}

## Instructions
1. Analyze the alert metadata to understand what triggered this issue.
2. Based on the source, severity, and metadata, determine the most likely root cause.
3. Consider common failure patterns for {severity}-severity {source} alerts.
4. Produce a structured RCA report in **Chinese (中文)** with the following markdown sections:

## Output Format

### 概要
1-2 句话总结根因。

### 时间线
按时间顺序列出关键事件。

### 根因分析
详细分析根本原因，包括：
- 直接原因
- 深层原因
- 触发条件

### 影响范围
- 受影响的服务、组件
- 用户影响程度

### 修复建议
#### 立即行动
- 紧急缓解措施

#### 长期改进
- 架构或流程层面的根本性改进

Be thorough but concise. Use bullet points. Include specific technical details from the metadata."#,
        title = issue.title,
        severity = issue.severity,
        source = issue.source,
        issue_type = issue.issue_type,
        description = description,
        meta = meta,
        created_at = issue.created_at,
    )
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

    let svc = ClaudeService::new(
        config.claude_bin.clone(),
        PathBuf::from(&config.claude_work_dir),
        Duration::from_millis(config.claude_timeout_ms),
        config.claude_model.clone(),
        10, // max_turns for RCA
        pool.clone(),
    );

    let stream_result = svc.run(
        &prompt,
        None,                // no session
        None,                // no system prompt override
        vec![],              // no images
        vec![],              // no extra env vars
        super::claude::AgentPermission::Bypass.cli_flag(),
        &[],                 // no disallowed tools
        &[],                 // no allowed tools
        None,                // no MCP config
    );

    let mut full_text = String::new();

    match stream_result {
        Ok(claude_stream) => {
            use tokio_stream::StreamExt;
            let mut stream = Box::pin(claude_stream);

            while let Some(chunk) = stream.next().await {
                // Accumulate text content
                match &chunk {
                    StreamChunk::Text { content } => {
                        full_text.push_str(content);
                    }
                    StreamChunk::Done { content, .. } => {
                        if !content.is_empty() && full_text.is_empty() {
                            full_text = content.clone();
                        }
                    }
                    _ => {}
                }

                // Broadcast to subscribers (ignore error = no subscribers)
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
