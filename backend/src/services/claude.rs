use serde::Serialize;
use sqlx::PgPool;
use std::path::PathBuf;
use std::time::Duration;
use uuid::Uuid;

/// Agent permission level — controls tool restrictions and sandbox mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentPermission {
    /// Block Write/Edit/NotebookEdit, Bash restricted to read-only
    Readonly,
    /// All tools allowed, sandboxed to CWD
    ReadWrite,
    /// Unrestricted
    Bypass,
}

impl AgentPermission {
    pub fn from_config(s: &str) -> Self {
        match s {
            "bypassPermissions" => Self::Bypass,
            "readwrite" => Self::ReadWrite,
            _ => Self::Readonly,
        }
    }

    /// Value for Claude CLI `--permission-mode` flag
    pub fn cli_flag(self) -> &'static str {
        match self {
            Self::Bypass => "bypassPermissions",
            _ => "default",
        }
    }
}

/// Stream chunk types matching Claude CLI stream-json output
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum StreamChunk {
    #[serde(rename = "init")]
    Init { session_id: Option<String> },
    #[serde(rename = "thinking")]
    Thinking { content: String },
    #[serde(rename = "text")]
    Text { content: String },
    #[serde(rename = "tool_use")]
    ToolUse { tool_name: String, content: String },
    #[serde(rename = "tool_result")]
    ToolResult { tool_name: String, content: String },
    #[serde(rename = "done")]
    Done {
        content: String,
        session_id: Option<String>,
        duration_ms: u64,
    },
    #[serde(rename = "error")]
    Error { message: String },
}

/// Claude Code CLI integration service.
/// Manages Claude CLI processes and persists sessions to database.
pub struct ClaudeService {
    pub claude_bin: String,
    pub work_dir: PathBuf,
    pub timeout: Duration,
    pub model: String,
    pub max_turns: u32,
    pub pool: PgPool,
}

impl ClaudeService {
    pub fn new(
        claude_bin: String,
        work_dir: PathBuf,
        timeout: Duration,
        model: String,
        max_turns: u32,
        pool: PgPool,
    ) -> Self {
        Self {
            claude_bin,
            work_dir,
            timeout,
            model,
            max_turns,
            pool,
        }
    }

    /// Find an active session for the user, or return None
    pub async fn find_active_session(&self, user_id: Uuid, tenant_id: Option<Uuid>) -> Option<String> {
        sqlx::query_scalar::<_, String>(
            r#"SELECT claude_session_id FROM claude_sessions
               WHERE user_id = $1
               AND ($2::UUID IS NULL OR tenant_id = $2)
               AND is_active = true
               AND last_active_at > NOW() - INTERVAL '30 minutes'
               ORDER BY last_active_at DESC
               LIMIT 1"#,
        )
        .bind(user_id)
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await
        .ok()
        .flatten()
    }

    /// Persist a new or updated session
    pub async fn save_session(
        pool: &PgPool,
        claude_session_id: &str,
        user_id: Uuid,
        tenant_id: Option<Uuid>,
        title: Option<&str>,
    ) -> anyhow::Result<()> {
        sqlx::query(
            r#"INSERT INTO claude_sessions (claude_session_id, user_id, tenant_id, title)
               VALUES ($1, $2, $3, $4)
               ON CONFLICT (claude_session_id) DO UPDATE
               SET last_active_at = NOW(), title = COALESCE($4, claude_sessions.title)"#,
        )
        .bind(claude_session_id)
        .bind(user_id)
        .bind(tenant_id)
        .bind(title)
        .execute(pool)
        .await?;

        Ok(())
    }

    /// Update session_id on a specific message (used for backfilling after Init).
    pub async fn backfill_message_session(
        pool: &PgPool,
        message_id: Uuid,
        session_id: &str,
    ) -> anyhow::Result<()> {
        sqlx::query("UPDATE chat_messages SET session_id = $1 WHERE id = $2")
            .bind(session_id)
            .bind(message_id)
            .execute(pool)
            .await?;
        Ok(())
    }

    /// Set duration_ms on a message.
    pub async fn set_message_duration(
        pool: &PgPool,
        message_id: Uuid,
        duration_ms: i64,
    ) -> anyhow::Result<()> {
        sqlx::query("UPDATE chat_messages SET duration_ms = $2 WHERE id = $1")
            .bind(message_id)
            .bind(duration_ms)
            .execute(pool)
            .await?;
        Ok(())
    }

    /// Insert a new chat message record.
    #[allow(clippy::too_many_arguments)]
    pub async fn save_message(
        pool: &PgPool,
        session_id: &str,
        role: &str,
        content: &str,
        msg_type: &str,
        tool_name: Option<&str>,
        images: Option<&serde_json::Value>,
        duration_ms: Option<i64>,
        seq: i32,
    ) -> anyhow::Result<Uuid> {
        let id = sqlx::query_scalar::<_, Uuid>(
            r#"INSERT INTO chat_messages (session_id, role, content, msg_type, tool_name, images, duration_ms, seq)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
               RETURNING id"#,
        )
        .bind(session_id)
        .bind(role)
        .bind(content)
        .bind(msg_type)
        .bind(tool_name)
        .bind(images)
        .bind(duration_ms)
        .bind(seq)
        .fetch_one(pool)
        .await?;
        Ok(id)
    }

    /// Append content to an existing message (used for streaming text chunks).
    pub async fn append_message_content(
        pool: &PgPool,
        id: Uuid,
        additional_content: &str,
    ) -> anyhow::Result<()> {
        sqlx::query("UPDATE chat_messages SET content = content || $2 WHERE id = $1")
            .bind(id)
            .bind(additional_content)
            .execute(pool)
            .await?;
        Ok(())
    }
}

impl StreamChunk {
    /// Convert an AgentEvent to a StreamChunk for SSE serialization
    pub fn from_agent_event(event: &crate::services::agent::AgentEvent) -> Self {
        use crate::services::agent::AgentEvent;
        match event {
            AgentEvent::Init { session_id } => StreamChunk::Init {
                session_id: session_id.clone(),
            },
            AgentEvent::Thinking { content } => StreamChunk::Thinking {
                content: content.clone(),
            },
            AgentEvent::Text { content } => StreamChunk::Text {
                content: content.clone(),
            },
            AgentEvent::ToolUse { tool_name, content } => StreamChunk::ToolUse {
                tool_name: tool_name.clone(),
                content: content.clone(),
            },
            AgentEvent::ToolResult { tool_name, content } => StreamChunk::ToolResult {
                tool_name: tool_name.clone(),
                content: content.clone(),
            },
            AgentEvent::Done {
                content,
                session_id,
                duration_ms,
            } => StreamChunk::Done {
                content: content.clone(),
                session_id: session_id.clone(),
                duration_ms: *duration_ms,
            },
            AgentEvent::Error { message } => StreamChunk::Error {
                message: message.clone(),
            },
        }
    }
}
