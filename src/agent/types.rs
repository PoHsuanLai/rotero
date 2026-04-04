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
    pub program: &'static str,
    pub args: &'static [&'static str],
}

pub const AGENT_PROVIDERS: &[AgentProvider] = &[
    AgentProvider {
        id: "claude",
        name: "Claude",
        description: "Anthropic Claude Code",
        program: "npx",
        args: &["--yes", "@agentclientprotocol/claude-agent-acp"],
    },
    AgentProvider {
        id: "gemini",
        name: "Gemini",
        description: "Google Gemini CLI",
        program: "npx",
        args: &["--yes", "@google/gemini-cli", "--acp"],
    },
    AgentProvider {
        id: "copilot",
        name: "GitHub Copilot",
        description: "GitHub Copilot CLI",
        program: "npx",
        args: &["--yes", "@github/copilot", "--acp"],
    },
    AgentProvider {
        id: "codex",
        name: "Codex",
        description: "OpenAI Codex CLI",
        program: "npx",
        args: &["--yes", "@zed-industries/codex-acp"],
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
