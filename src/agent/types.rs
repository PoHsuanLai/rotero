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
    },
    Error(String),
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
    /// Entry point relative to package (node runs this directly).
    pub entry_point: &'static str,
    /// Extra args to pass after the entry point.
    pub extra_args: &'static [&'static str],
}

pub const AGENT_PROVIDERS: &[AgentProvider] = &[
    AgentProvider {
        id: "claude",
        name: "Claude",
        description: "Anthropic Claude Code",
        npm_package: "@agentclientprotocol/claude-agent-acp",
        entry_point: "dist/index.js",
        extra_args: &[],
    },
    AgentProvider {
        id: "gemini",
        name: "Gemini",
        description: "Google Gemini CLI",
        npm_package: "@google/gemini-cli",
        entry_point: "dist/cli.js",
        extra_args: &["--acp"],
    },
    AgentProvider {
        id: "copilot",
        name: "GitHub Copilot",
        description: "GitHub Copilot CLI",
        npm_package: "@github/copilot",
        entry_point: "dist/cli.js",
        extra_args: &["--acp"],
    },
    AgentProvider {
        id: "codex",
        name: "Codex",
        description: "OpenAI Codex CLI",
        npm_package: "@zed-industries/codex-acp",
        entry_point: "dist/index.js",
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

/// A past session that can be resumed.
#[derive(Debug, Clone, PartialEq)]
pub struct PastSession {
    pub session_id: String,
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
}

/// Messages sent from UI -> agent thread.
pub enum ChatRequest {
    SendMessage {
        prompt: String,
        paper_context: Option<String>,
    },
    Cancel,
    Authenticate { method_id: String },
    ListSessions,
    LoadSession { session_id: String },
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
    },
    SessionCreated,
    TextDelta(String),
    ToolCallStarted { id: String, title: String },
    ToolCallUpdated { id: String, status: ToolStatus },
    TurnCompleted,
    CommandsAvailable(Vec<SlashCommand>),
    SessionList(Vec<PastSession>),
    Error(String),
}
