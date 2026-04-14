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
    pub hidden: bool,
}

impl ChatMessage {
    pub fn new(role: ChatRole, content: Vec<MessageContent>) -> Self {
        Self {
            role,
            content,
            timestamp: Utc::now(),
            hidden: false,
        }
    }

    pub fn assistant(content: Vec<MessageContent>) -> Self {
        Self::new(ChatRole::Assistant, content)
    }

    pub fn hidden(role: ChatRole, content: Vec<MessageContent>) -> Self {
        Self {
            role,
            content,
            timestamp: Utc::now(),
            hidden: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum AgentStatus {
    #[default]
    Idle,
    Connecting,
    Streaming,
    ToolCall(String),
    NeedsAuth,
    Error(String),
    #[allow(dead_code)]
    NotInstalled,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentProvider {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub npm_package: &'static str,
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

#[derive(Debug, Clone, PartialEq)]
pub struct SlashCommand {
    pub name: String,
    pub description: String,
    pub hint: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentModel {
    pub id: String,
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PastSession {
    pub session_id: String,
    pub cwd: String,
    pub title: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentAuthMethod {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub terminal_command: Option<String>,
    pub terminal_args: Vec<String>,
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
    pub active_provider_id: String,
    pub supports_list_sessions: bool,
    pub available_models: Vec<AgentModel>,
    pub current_model: String,
}

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
    Authenticate {
        method_id: String,
    },
    SetModel {
        model_id: String,
    },
    ListSessions,
    LoadSession {
        session_id: String,
        cwd: String,
    },
    SwitchAgent {
        provider_id: String,
    },
    #[allow(dead_code)]
    Shutdown,
}

#[derive(Debug)]
pub enum ChatEvent {
    Switching {
        provider_id: String,
    },
    Connected {
        auth_methods: Vec<AgentAuthMethod>,
        provider_id: String,
        supports_list_sessions: bool,
    },
    SessionCreated,
    UserMessage(String),
    TextDelta(String),
    ToolCallStarted {
        id: String,
        title: String,
    },
    PermissionRequest {
        request_id: serde_json::Value,
        tool_title: String,
        options: Vec<(String, String)>,
    },
    ToolCallUpdated {
        id: String,
        status: ToolStatus,
        output: Option<String>,
    },
    TurnCompleted,
    CommandsAvailable(Vec<SlashCommand>),
    ModelsAvailable {
        models: Vec<AgentModel>,
        current: String,
    },
    SessionList(Vec<PastSession>),
    AuthRequired {
        provider_name: String,
    },
    Error(String),
}
