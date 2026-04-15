use std::path::PathBuf;
use std::time::Duration;
use serde::Deserialize;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

use super::{Agent, AgentEvent, AgentSessionConfig, ImageData};

/// Claude Code CLI agent implementation.
pub struct ClaudeAgent {
    pub bin_path: String,
    pub work_dir: PathBuf,
    pub timeout: Duration,
}

impl Agent for ClaudeAgent {
    fn name(&self) -> &str {
        "claude-code"
    }

    fn run(
        &self,
        config: AgentSessionConfig,
    ) -> Result<mpsc::Receiver<AgentEvent>, anyhow::Error> {
        let (tx, rx) = mpsc::channel(64);

        let has_images = !config.images.is_empty();

        // Only pass --resume if the Claude CLI session file actually exists on disk.
        // After pod restarts, DB may have session records but Claude CLI's local
        // conversation files are gone — passing --resume would fail with
        // "No conversation found with session ID".
        let effective_session_id = config.session_id.as_deref().and_then(|sid| {
            let session_file = self.work_dir.join(".claude").join("projects").join(sid);
            if session_file.exists() {
                Some(sid)
            } else {
                // Also check the standard Claude Code session storage path
                let alt_path = self.work_dir.join(".claude").join("conversations").join(sid);
                if alt_path.exists() {
                    Some(sid)
                } else {
                    tracing::warn!(
                        "Claude session {} not found on disk, starting fresh (DB session may be stale after pod restart)",
                        sid
                    );
                    None
                }
            }
        });

        // Build CLI args
        let args = build_args(
            &config.message,
            effective_session_id,
            config.system_prompt.as_deref(),
            has_images,
            &config.permission_mode,
            &config.disallowed_tools,
            &config.allowed_tools,
            config.mcp_config_path.as_deref(),
            &config.model,
            config.max_turns,
        );

        let timeout = self.timeout;

        tracing::info!(
            "ClaudeAgent: spawning model={}, {} images, args={:?} in {:?}",
            config.model,
            config.images.len(),
            &args,
            self.work_dir
        );

        // Build stdin payload for multimodal
        let stdin_data = if has_images {
            Some(build_stream_input(&config.message, &config.images))
        } else {
            None
        };

        let stdin_mode = if has_images {
            std::process::Stdio::piped()
        } else {
            std::process::Stdio::null()
        };

        let mut cmd = Command::new(&self.bin_path);
        cmd.args(&args)
            .current_dir(&self.work_dir)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .stdin(stdin_mode)
            .kill_on_drop(true);

        // Remove inherited env vars that interfere
        cmd.env_remove("AWS_BEARER_TOKEN_BEDROCK");
        cmd.env_remove("AWS_BEARER_TOKEN");
        cmd.env_remove("CLAUDE_CODE_USE_BEDROCK");
        cmd.env_remove("ANTHROPIC_BASE_URL");
        cmd.env_remove("ANTHROPIC_API_KEY");

        for (key, value) in &config.env_vars {
            cmd.env(key, value);
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| anyhow::anyhow!("Failed to spawn claude: {}", e))?;
        let child_stdin = child.stdin.take();
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("No stdout"))?;
        let stderr = child.stderr.take();

        // Spawn the reader task -- it pushes events into the channel
        tokio::spawn(async move {
            // Write stdin first (for images)
            if let (Some(data), Some(mut stdin)) = (stdin_data, child_stdin) {
                tracing::info!("Writing {} bytes to claude stdin", data.len());
                if let Err(e) = stdin.write_all(data.as_bytes()).await {
                    let _ = tx
                        .send(AgentEvent::Error {
                            message: format!("Failed to send images: {}", e),
                        })
                        .await;
                    let _ = child.kill().await;
                    return;
                }
                let _ = stdin.write_all(b"\n").await;
                drop(stdin);
            }

            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            let start = std::time::Instant::now();

            loop {
                if start.elapsed() > timeout {
                    let _ = tx
                        .send(AgentEvent::Error {
                            message: "Claude CLI timeout".to_string(),
                        })
                        .await;
                    let _ = child.kill().await;
                    break;
                }

                match tokio::time::timeout(Duration::from_secs(60), lines.next_line()).await {
                    Ok(Ok(Some(line))) => {
                        if line.trim().is_empty() {
                            continue;
                        }

                        // Debug logging
                        if let Ok(raw) = serde_json::from_str::<serde_json::Value>(&line) {
                            let etype =
                                raw.get("type").and_then(|v| v.as_str()).unwrap_or("?");
                            let subtype = raw.get("subtype").and_then(|v| v.as_str());
                            tracing::info!(
                                "Claude CLI event: type={} subtype={:?}",
                                etype,
                                subtype
                            );
                            if etype == "user" {
                                let preview = if line.len() > 500 {
                                    &line[..500]
                                } else {
                                    &line
                                };
                                tracing::info!("Claude CLI user event: {}", preview);
                            }
                        }

                        let events = parse_stream_line(&line);
                        let mut is_done = false;
                        for event in events {
                            if matches!(&event, AgentEvent::Done { .. }) {
                                is_done = true;
                            }
                            if tx.send(event).await.is_err() {
                                // Consumer dropped
                                let _ = child.kill().await;
                                return;
                            }
                        }
                        if is_done {
                            break;
                        }
                    }
                    Ok(Ok(None)) => break, // EOF
                    Ok(Err(e)) => {
                        let _ = tx
                            .send(AgentEvent::Error {
                                message: format!("Read error: {}", e),
                            })
                            .await;
                        break;
                    }
                    Err(_) => continue, // 60s timeout, still waiting
                }
            }

            // Read stderr
            if let Some(stderr) = stderr {
                let mut stderr_reader = BufReader::new(stderr);
                let mut buf = String::new();
                let _ =
                    tokio::io::AsyncReadExt::read_to_string(&mut stderr_reader, &mut buf).await;
                if !buf.is_empty() {
                    tracing::warn!("Claude stderr: {}", &buf[..buf.len().min(2000)]);
                }
            }

            let _ = child.wait().await;
        });

        Ok(rx)
    }
}

// ─── Internal deserialization structs ────────────────────────────────────────

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
    #[serde(default)]
    #[allow(dead_code)]
    tool_name: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
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
    /// MCP tool_result blocks use "content" instead of "text" for the result string
    #[serde(default)]
    content: Option<serde_json::Value>,
    /// tool_name from tool_reference blocks
    #[serde(default)]
    tool_name: Option<String>,
}

// ─── Helper functions ────────────────────────────────────────────────────────

/// Build Claude CLI arguments.
/// When `use_stream_input` is true, uses `--input-format stream-json` and reads from stdin
/// instead of passing the message as a positional argument (needed for images).
#[allow(clippy::too_many_arguments)]
fn build_args(
    message: &str,
    session_id: Option<&str>,
    system_prompt: Option<&str>,
    use_stream_input: bool,
    permission_mode: &str,
    disallowed_tools: &[String],
    allowed_tools: &[String],
    mcp_config: Option<&str>,
    model: &str,
    max_turns: u32,
) -> Vec<String> {
    let mut args = vec!["-p".to_string()];

    if !use_stream_input {
        args.push(message.to_string());
    }

    args.extend([
        "--output-format".to_string(),
        "stream-json".to_string(),
        "--verbose".to_string(),
        "--max-turns".to_string(),
        max_turns.to_string(),
        "--model".to_string(),
        model.to_string(),
        "--permission-mode".to_string(),
        permission_mode.to_string(),
    ]);

    if !disallowed_tools.is_empty() {
        args.push("--disallowedTools".to_string());
        args.push(disallowed_tools.join(","));
    }

    if !allowed_tools.is_empty() {
        args.push("--allowedTools".to_string());
        args.push(allowed_tools.join(","));
    }

    if let Some(cfg) = mcp_config {
        args.push("--mcp-config".to_string());
        args.push(cfg.to_string());
    }

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
fn build_stream_input(message: &str, images: &[ImageData]) -> String {
    let mut content = Vec::new();

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

/// Parse a single line of Claude stream-json output into AgentEvent(s)
pub fn parse_stream_line(line: &str) -> Vec<AgentEvent> {
    let mut events = Vec::new();

    let event: ClaudeEvent = match serde_json::from_str(line) {
        Ok(e) => e,
        Err(_) => return events,
    };

    tracing::debug!(
        "Claude CLI event: type={} subtype={:?}",
        event.event_type,
        event.subtype
    );

    match event.event_type.as_str() {
        "system" if event.subtype.as_deref() == Some("init") => {
            events.push(AgentEvent::Init {
                session_id: event.session_id,
            });
        }
        "assistant" => {
            if let Some(msg) = event.message {
                // Reorder: thinking → text → tool_use → tool_result
                // This ensures text appears before tool calls in the chat UI
                let mut thinking_events = Vec::new();
                let mut text_events = Vec::new();
                let mut tool_events = Vec::new();

                for block in msg.content {
                    match block.block_type.as_str() {
                        "thinking" => {
                            if let Some(thinking) = block.thinking {
                                thinking_events
                                    .push(AgentEvent::Thinking { content: thinking });
                            }
                        }
                        "text" => {
                            if let Some(text) = block.text {
                                text_events.push(AgentEvent::Text { content: text });
                            }
                        }
                        "tool_use" => {
                            let name =
                                block.name.unwrap_or_else(|| "unknown".to_string());
                            let input_str = block
                                .input
                                .map(|v| {
                                    serde_json::to_string_pretty(&v).unwrap_or_default()
                                })
                                .unwrap_or_default();
                            tool_events.push(AgentEvent::ToolUse {
                                tool_name: name,
                                content: input_str,
                            });
                        }
                        "tool_result" => {
                            let content = block.text.unwrap_or_default();
                            let name =
                                block.name.unwrap_or_else(|| "tool".to_string());
                            tool_events.push(AgentEvent::ToolResult {
                                tool_name: name,
                                content,
                            });
                        }
                        _ => {}
                    }
                }

                events.extend(thinking_events);
                events.extend(text_events);
                events.extend(tool_events);
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

            events.push(AgentEvent::Done {
                content,
                session_id: event.session_id,
                duration_ms: event.duration_ms.unwrap_or(0),
            });
        }
        "user" => {
            if let Some(msg) = event.message {
                for block in msg.content {
                    if block.block_type == "tool_result" {
                        let result_str = match &block.content {
                            Some(serde_json::Value::String(s)) => Some(s.clone()),
                            _ => block.text.clone(),
                        };
                        let name = block
                            .tool_name
                            .or(block.name)
                            .unwrap_or_else(|| "mcp_tool".to_string());
                        if let Some(content) = result_str
                            && !content.is_empty()
                        {
                            events.push(AgentEvent::ToolResult {
                                tool_name: name,
                                content,
                            });
                        }
                    }
                }
            }
        }
        other => {
            tracing::debug!("Unhandled Claude CLI event type: {}", other);
        }
    }

    events
}
