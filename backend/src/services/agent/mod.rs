pub mod claude;

use tokio::sync::mpsc;

/// Events produced by an AI agent during a conversation.
/// These are pure domain events -- no HTTP/serde dependency.
#[derive(Debug, Clone)]
pub enum AgentEvent {
    /// Session initialized with an ID
    Init { session_id: Option<String> },
    /// Thinking/reasoning content
    Thinking { content: String },
    /// Text response content
    Text { content: String },
    /// Tool being used
    ToolUse { tool_name: String, content: String },
    /// Tool execution result
    ToolResult { tool_name: String, content: String },
    /// Conversation turn complete
    Done {
        content: String,
        session_id: Option<String>,
        duration_ms: u64,
    },
    /// Error occurred
    Error { message: String },
}

/// Image data for multimodal input
pub struct ImageData {
    pub media_type: String,
    pub data: String, // base64
}

/// Configuration for an agent session
pub struct AgentSessionConfig {
    pub session_id: Option<String>,
    pub message: String,
    pub system_prompt: Option<String>,
    pub model: String,
    pub max_turns: u32,
    pub permission_mode: String,
    pub allowed_tools: Vec<String>,
    pub disallowed_tools: Vec<String>,
    pub env_vars: Vec<(String, String)>,
    pub mcp_config_path: Option<String>,
    pub images: Vec<ImageData>,
}

/// Core Agent trait -- any AI backend implements this.
///
/// The `run()` method spawns the agent process and returns a channel receiver.
/// The producer (CLI read loop) and consumer (SSE handler) are fully decoupled.
pub trait Agent: Send + Sync {
    /// Start a conversation. Returns a receiver for streaming events.
    fn run(
        &self,
        config: AgentSessionConfig,
    ) -> Result<mpsc::Receiver<AgentEvent>, anyhow::Error>;

    /// Agent name (for logging and selection)
    fn name(&self) -> &str;
}
