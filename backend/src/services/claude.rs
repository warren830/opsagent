use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::path::PathBuf;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio_stream::Stream;
use uuid::Uuid;

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

#[derive(Debug, Deserialize)]
struct ClaudeEvent {
    #[serde(rename = "type")]
    event_type: String,
    #[serde(default)]
    subtype: Option<String>,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    message: Option<ClaudeMessage>,
    #[serde(default)]
    result: Option<serde_json::Value>,
    #[serde(default)]
    duration_ms: Option<u64>,
    // tool_result fields
    #[serde(default)]
    tool_name: Option<String>,
    #[serde(default)]
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ClaudeMessage {
    #[serde(default)]
    content: Vec<ClaudeContentBlock>,
}

#[derive(Debug, Deserialize)]
struct ClaudeContentBlock {
    #[serde(rename = "type")]
    block_type: String,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    thinking: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    input: Option<serde_json::Value>,
}

/// Image data to send to Claude via stream-json input
#[derive(Debug, Clone, Serialize)]
pub struct ChatImageData {
    pub data: String,       // base64
    pub media_type: String, // e.g. "image/png"
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
    pub async fn find_active_session(
        &self,
        user_id: Uuid,
        tenant_id: Option<Uuid>,
    ) -> Option<String> {
        let row = sqlx::query_scalar::<_, String>(
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
        .flatten();

        row
    }

    /// Persist a new or updated session
    pub async fn save_session(
        &self,
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
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Build Claude CLI arguments.
    /// When `use_stream_input` is true, uses `--input-format stream-json` and reads from stdin
    /// instead of passing the message as a positional argument (needed for images).
    /// Skills are discovered natively by Claude CLI from `.claude/skills/` in `work_dir`.
    pub fn build_args(
        &self,
        message: &str,
        session_id: Option<&str>,
        system_prompt: Option<&str>,
        use_stream_input: bool,
    ) -> Vec<String> {
        let mut args = vec!["-p".to_string()];

        if !use_stream_input {
            // Simple text mode: pass message as argument
            args.push(message.to_string());
        }

        args.extend([
            "--output-format".to_string(),
            "stream-json".to_string(),
            "--verbose".to_string(),
            "--max-turns".to_string(),
            self.max_turns.to_string(),
            "--model".to_string(),
            self.model.clone(),
            "--permission-mode".to_string(),
            "bypassPermissions".to_string(),
        ]);

        if use_stream_input {
            args.push("--input-format".to_string());
            args.push("stream-json".to_string());
        }

        if let Some(sp) = system_prompt {
            args.push("--system-prompt".to_string());
            args.push(sp.to_string());
        }

        if let Some(sid) = session_id {
            args.push("--resume".to_string());
            args.push(sid.to_string());
        }

        args
    }

    /// Build the stream-json input message containing text + optional images.
    /// Format: {"type":"user","message":{"role":"user","content":[...]}}
    fn build_stream_input(message: &str, images: &[ChatImageData]) -> String {
        let mut content = Vec::new();

        // Add image blocks first
        for img in images {
            content.push(serde_json::json!({
                "type": "image",
                "source": {
                    "type": "base64",
                    "media_type": img.media_type,
                    "data": img.data,
                }
            }));
        }

        // Add text block
        content.push(serde_json::json!({
            "type": "text",
            "text": message,
        }));

        let msg = serde_json::json!({
            "type": "user",
            "message": {
                "role": "user",
                "content": content,
            }
        });

        serde_json::to_string(&msg).unwrap_or_default()
    }

    /// Run Claude CLI and return a stream of parsed chunks.
    /// This method consumes self to ensure the returned stream is 'static.
    /// When `images` is non-empty, uses `--input-format stream-json` to pipe
    /// multimodal content via stdin (base64 images + text).
    /// `env_vars` are injected into the child process (e.g. AWS_PROFILE, AWS_ROLE_ARN).
    pub fn run(
        self,
        message: &str,
        session_id: Option<&str>,
        system_prompt: Option<&str>,
        images: Vec<ChatImageData>,
        env_vars: Vec<(String, String)>,
    ) -> Result<impl Stream<Item = StreamChunk> + Send + 'static, std::io::Error> {
        let has_images = !images.is_empty();
        let args = self.build_args(message, session_id, system_prompt, has_images);
        let timeout = self.timeout;

        tracing::info!(
            "Spawning claude with {} images, {} env vars, args: {:?} in {:?}",
            images.len(),
            env_vars.len(),
            &args[..3.min(args.len())],
            self.work_dir
        );

        let stdin_mode = if has_images {
            std::process::Stdio::piped()
        } else {
            std::process::Stdio::null()
        };

        // Build stdin payload before spawning (if images)
        let stdin_data = if has_images {
            Some(Self::build_stream_input(message, &images))
        } else {
            None
        };

        let mut cmd = Command::new(&self.claude_bin);
        cmd.args(&args)
            .current_dir(&self.work_dir)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .stdin(stdin_mode)
            .kill_on_drop(true);

        // Inject environment variables (e.g. AWS_PROFILE, AWS_ROLE_ARN)
        for (key, value) in &env_vars {
            cmd.env(key, value);
        }

        let mut child = cmd.spawn()?;

        // Take stdin handle before moving child into stream
        let child_stdin = child.stdin.take();

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::Other, "No stdout"))?;

        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();

        let stream = async_stream::stream! {
            // Write stdin FIRST (inside the stream so it's guaranteed to happen before reads)
            if let (Some(data), Some(mut stdin)) = (stdin_data, child_stdin) {
                tracing::info!("Writing {} bytes to claude stdin (stream-json with images)", data.len());
                if let Err(e) = stdin.write_all(data.as_bytes()).await {
                    tracing::error!("Failed to write to claude stdin: {}", e);
                    yield StreamChunk::Error { message: format!("Failed to send images: {}", e) };
                    let _ = child.kill().await;
                    return;
                }
                if let Err(e) = stdin.write_all(b"\n").await {
                    tracing::error!("Failed to write newline to claude stdin: {}", e);
                }
                drop(stdin); // close stdin so Claude starts processing
                tracing::info!("Stdin written and closed, waiting for claude output...");
            }

            let start = std::time::Instant::now();

            loop {
                if start.elapsed() > timeout {
                    yield StreamChunk::Error {
                        message: "Claude CLI timeout exceeded".to_string(),
                    };
                    let _ = child.kill().await;
                    break;
                }

                match tokio::time::timeout(Duration::from_secs(60), lines.next_line()).await {
                    Ok(Ok(Some(line))) => {
                        if line.trim().is_empty() {
                            continue;
                        }
                        let chunks = Self::parse_stream_line(&line);
                        let mut is_done = false;
                        for chunk in chunks {
                            if matches!(&chunk, StreamChunk::Done { .. }) {
                                is_done = true;
                            }
                            yield chunk;
                        }
                        if is_done {
                            break;
                        }
                    }
                    Ok(Ok(None)) => {
                        // EOF
                        break;
                    }
                    Ok(Err(e)) => {
                        yield StreamChunk::Error {
                            message: format!("Read error: {}", e),
                        };
                        break;
                    }
                    Err(_) => {
                        // 60s line timeout — still waiting, continue
                        continue;
                    }
                }
            }

            // Ensure child is cleaned up
            let _ = child.wait().await;
        };

        Ok(stream)
    }

    /// Parse a single line of Claude stream-json output into StreamChunk(s)
    pub fn parse_stream_line(line: &str) -> Vec<StreamChunk> {
        let mut chunks = Vec::new();

        let event: ClaudeEvent = match serde_json::from_str(line) {
            Ok(e) => e,
            Err(_) => return chunks,
        };

        match event.event_type.as_str() {
            "system" if event.subtype.as_deref() == Some("init") => {
                chunks.push(StreamChunk::Init {
                    session_id: event.session_id,
                });
            }
            "assistant" => {
                if let Some(msg) = event.message {
                    for block in msg.content {
                        match block.block_type.as_str() {
                            "thinking" => {
                                if let Some(thinking) = block.thinking {
                                    chunks.push(StreamChunk::Thinking { content: thinking });
                                }
                            }
                            "text" => {
                                if let Some(text) = block.text {
                                    chunks.push(StreamChunk::Text { content: text });
                                }
                            }
                            "tool_use" => {
                                let name =
                                    block.name.unwrap_or_else(|| "unknown".to_string());
                                let input_str = block
                                    .input
                                    .map(|v| serde_json::to_string_pretty(&v).unwrap_or_default())
                                    .unwrap_or_default();
                                chunks.push(StreamChunk::ToolUse {
                                    tool_name: name,
                                    content: input_str,
                                });
                            }
                            "tool_result" => {
                                let content = block.text.unwrap_or_default();
                                let name =
                                    block.name.unwrap_or_else(|| "tool".to_string());
                                chunks.push(StreamChunk::ToolResult {
                                    tool_name: name,
                                    content,
                                });
                            }
                            _ => {}
                        }
                    }
                }
            }
            "result" => {
                let content = event
                    .result
                    .map(|r| {
                        if let serde_json::Value::String(s) = r {
                            s
                        } else {
                            serde_json::to_string(&r).unwrap_or_default()
                        }
                    })
                    .unwrap_or_default();

                chunks.push(StreamChunk::Done {
                    content,
                    session_id: event.session_id,
                    duration_ms: event.duration_ms.unwrap_or(0),
                });
            }
            _ => {}
        }

        chunks
    }
}
