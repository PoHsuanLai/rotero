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
    /// Env var keys this provider needs (e.g. ["GEMINI_API_KEY"]).
    /// Empty for providers that use CLI-managed auth (Claude).
    pub env_keys: &'static [&'static str],
    /// Hint text shown in the API key input field.
    pub env_hint: &'static str,
    /// Extra env vars to always set when spawning this provider.
    pub extra_env: &'static [(&'static str, &'static str)],
}

pub const AGENT_PROVIDERS: &[AgentProvider] = &[
    AgentProvider {
        id: "claude",
        name: "Claude",
        description: "Anthropic Claude Code (requires Claude subscription)",
        program: "npx",
        args: &["--yes", "@agentclientprotocol/claude-agent-acp"],
        env_keys: &[],
        env_hint: "",
        extra_env: &[],
    },
    AgentProvider {
        id: "gemini",
        name: "Gemini",
        description: "Google Gemini CLI (requires Google account or API key)",
        program: "npx",
        args: &["--yes", "@google/gemini-cli", "--acp"],
        env_keys: &["GEMINI_API_KEY"],
        env_hint: "AIza... (from ai.google.dev)",
        extra_env: &[],
    },
    AgentProvider {
        id: "copilot",
        name: "GitHub Copilot",
        description: "GitHub Copilot (requires Copilot subscription)",
        program: "npx",
        args: &["--yes", "@github/copilot", "--acp"],
        env_keys: &["GITHUB_TOKEN"],
        env_hint: "ghp_... or fine-grained PAT with Copilot access",
        extra_env: &[],
    },
    AgentProvider {
        id: "codex",
        name: "Codex",
        description: "OpenAI Codex (requires OpenAI API key)",
        program: "npx",
        args: &["--yes", "@zed-industries/codex-acp"],
        env_keys: &["OPENAI_API_KEY"],
        env_hint: "sk-... (from platform.openai.com)",
        extra_env: &[("NO_BROWSER", "1")],
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
