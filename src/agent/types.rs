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
}

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
    pub is_api_key: bool,
    /// The env var name for API key methods (e.g. "XAI_API_KEY").
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
    pub active_provider_name: String,
    pub supports_list_sessions: bool,
    pub available_models: Vec<AgentModel>,
    pub current_model: String,
    /// Config-option id for the model picker, when the agent uses `configOptions`.
    pub model_config_id: Option<String>,
    /// The agent session backing the visible transcript, once one exists.
    pub current_session_id: Option<String>,
    /// What the next session created will be about. Set when a message is sent,
    /// since the subject is known then but the session id is not yet.
    pub pending_subject: Option<rotero_db::chat_sessions::ChatSubject>,
    /// What the visible conversation is about, once it has been resumed or
    /// started for a subject.
    pub current_subject: Option<rotero_db::chat_sessions::ChatSubject>,
    /// A subject the panel would switch to, waiting on the user's answer
    /// because switching now would abandon a conversation in progress.
    pub pending_switch: Option<PendingSwitch>,
    /// A subject the user declined to switch to, so the offer is not repeated
    /// until they move somewhere else.
    pub declined_subject: Option<rotero_db::chat_sessions::ChatSubject>,
    /// Show every past chat rather than only the current subject's.
    ///
    /// The list is about what is open, so it filters by default; this widens it
    /// to reach a conversation about something else — including the ones that
    /// belong to no paper at all.
    pub browse_all_sessions: bool,
}

/// A subject switch the user has been asked about but not yet answered.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingSwitch {
    pub subject: rotero_db::chat_sessions::ChatSubject,
    /// What to call it in the prompt.
    pub label: String,
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
    /// Ask for a one-line description of the conversation so far.
    ///
    /// The reply is routed to [`ChatEvent::SessionSummary`] and never reaches
    /// the transcript.
    SummarizeSession {
        session_id: String,
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
        provider_name: String,
        supports_list_sessions: bool,
    },
    SessionCreated {
        session_id: String,
    },
    UserMessage {
        text: String,
        /// Papers named in the message's `<rotero-context>` block, recovered
        /// before the block was stripped for display.
        context_paper_ids: Vec<String>,
    },
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
        config_id: Option<String>,
    },
    SessionList(Vec<PastSession>),
    /// A one-line description of a conversation, for surfaces that show it
    /// without its transcript. Carries no visible message.
    SessionSummary {
        session_id: String,
        summary: String,
    },
    /// The agent could not load a conversation, so it no longer exists as far
    /// as this device is concerned.
    SessionLoadFailed {
        session_id: String,
    },
    AuthRequired {
        provider_name: String,
    },
    Error(String),
}
