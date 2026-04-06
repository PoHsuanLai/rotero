use chrono::{DateTime, Utc};

#[derive(Debug, Clone, PartialEq)]
pub enum ChatRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MessageContent {
    Text(String),
    ToolUse {
        id: String,
        title: String,
        status: ToolStatus,
        output: Option<String>,
    },
    Error(String),
    /// Permission request shown as inline buttons.
    Permission {
        request_id: serde_json::Value,
        tool_title: String,
        options: Vec<(String, String)>, // (optionId, label)
        responded: bool,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum ToolStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: Vec<MessageContent>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum AgentStatus {
    #[default]
    Idle,
    Connecting,
    Streaming,
    ToolCall(String),
    /// Needs sign-in before use — not an error, just informational.
    NeedsAuth,
    Error(String),
    NotInstalled,
}

/// A known ACP agent provider.
#[derive(Debug, Clone, PartialEq)]
pub struct AgentProvider {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    /// npm package name to install.
    pub npm_package: &'static str,
    /// Extra args to pass after the entry point.
    pub extra_args: &'static [&'static str],
}

pub const AGENT_PROVIDERS: &[AgentProvider] = &[
    AgentProvider {
        id: "claude",
        name: "Claude",
        description: "Anthropic Claude Code",
        npm_package: "@agentclientprotocol/claude-agent-acp",
        extra_args: &[],
    },
    AgentProvider {
        id: "gemini",
        name: "Gemini",
        description: "Google Gemini CLI",
        npm_package: "@google/gemini-cli",
        extra_args: &["--acp"],
    },
    AgentProvider {
        id: "copilot",
        name: "GitHub Copilot",
        description: "GitHub Copilot CLI",
        npm_package: "@github/copilot",
        extra_args: &["--acp"],
    },
    AgentProvider {
        id: "codex",
        name: "Codex",
        description: "OpenAI Codex CLI",
        npm_package: "@zed-industries/codex-acp",
        extra_args: &[],
    },
];

/// A slash command exposed by the agent.
#[derive(Debug, Clone, PartialEq)]
pub struct SlashCommand {
    pub name: String,
    pub description: String,
    pub hint: Option<String>,
}

/// An available model from the agent.
#[derive(Debug, Clone, PartialEq)]
pub struct AgentModel {
    pub id: String,
    pub name: String,
    pub description: String,
}

/// A past session that can be resumed.
#[derive(Debug, Clone, PartialEq)]
pub struct PastSession {
    pub session_id: String,
    pub cwd: String,
    pub title: Option<String>,
    pub updated_at: Option<String>,
}

/// Auth method advertised by the agent during initialization.
#[derive(Debug, Clone, PartialEq)]
pub struct AgentAuthMethod {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub terminal_command: Option<String>,
    pub terminal_args: Vec<String>,
    /// True if this is an API key method (needs env var, not browser).
    pub is_api_key: bool,
    /// The env var name for API key methods (e.g. "GEMINI_API_KEY").
    pub api_key_env_var: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ChatState {
    pub messages: Vec<ChatMessage>,
    pub status: AgentStatus,
    pub input_text: String,
    pub panel_open: bool,
    pub session_active: bool,
    pub commands: Vec<SlashCommand>,
    pub show_command_picker: bool,
    pub past_sessions: Vec<PastSession>,
    pub show_session_browser: bool,
    pub auth_methods: Vec<AgentAuthMethod>,
    /// The provider id that is actually connected right now.
    pub active_provider_id: String,
    /// Whether the connected agent supports listing past sessions.
    pub supports_list_sessions: bool,
    /// Available models for the current provider.
    pub available_models: Vec<AgentModel>,
    /// Currently selected model id.
    pub current_model: String,
}

/// Messages sent from UI -> agent thread.
pub enum ChatRequest {
    SendMessage {
        prompt: String,
        paper_context: Option<String>,
    },
    Cancel,
    PermissionResponse {
        request_id: serde_json::Value,
        option_id: String,
    },
    Authenticate { method_id: String },
    SetModel { model_id: String },
    ListSessions,
    LoadSession { session_id: String, cwd: String },
    SwitchAgent { provider_id: String },
    Shutdown,
}

/// Events sent from agent thread -> UI.
#[derive(Debug)]
pub enum ChatEvent {
    /// Agent is switching — sent immediately before teardown for instant UI feedback.
    Switching { provider_id: String },
    Connected {
        auth_methods: Vec<AgentAuthMethod>,
        provider_id: String,
        supports_list_sessions: bool,
    },
    SessionCreated,
    UserMessage(String),
    TextDelta(String),
    ToolCallStarted { id: String, title: String },
    /// Agent asks for permission to run a tool.
    PermissionRequest {
        request_id: serde_json::Value,
        tool_title: String,
        options: Vec<(String, String)>, // (optionId, label)
    },
    ToolCallUpdated { id: String, status: ToolStatus, output: Option<String> },
    TurnCompleted,
    CommandsAvailable(Vec<SlashCommand>),
    ModelsAvailable { models: Vec<AgentModel>, current: String },
    SessionList(Vec<PastSession>),
    /// Auth needed — informational, not an error.
    AuthRequired { provider_name: String },
    Error(String),
}
