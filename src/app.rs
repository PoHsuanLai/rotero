use dioxus::prelude::*;

use crate::agent::types::{AgentStatus, ChatEvent, ChatMessage, ChatRole, ChatState, MessageContent};
use crate::db::Database;
use crate::state::app_state::{DragPaper, LibraryState, PdfTabManager, ViewerToolState};
use crate::state::commands;
use crate::sync::engine::SyncConfig;
use crate::ui::chat_panel::AgentChannel;
use crate::ui::layout::Layout;

/// Global signal wrapper for settings dialog visibility.
#[derive(Debug, Clone, Copy, Default)]
pub struct ShowSettings(pub bool);

const FONTS_CSS: &str = include_str!("../assets/fonts.css");
const TOKENS_CSS: &str = include_str!("../assets/tokens.css");
const BASE_CSS: &str = include_str!("../assets/base.css");
const BUTTONS_CSS: &str = include_str!("../assets/buttons.css");
const INPUTS_CSS: &str = include_str!("../assets/inputs.css");
const LAYOUT_CSS: &str = include_str!("../assets/layout.css");
const SIDEBAR_CSS: &str = include_str!("../assets/sidebar.css");
const LIBRARY_CSS: &str = include_str!("../assets/library.css");
const DETAIL_CSS: &str = include_str!("../assets/detail.css");
const PDF_CSS: &str = include_str!("../assets/pdf.css");
const COMPONENTS_CSS: &str = include_str!("../assets/components.css");
const DIALOGS_CSS: &str = include_str!("../assets/dialogs.css");
const THEME_CSS: &str = include_str!("../assets/theme.css");
const CHAT_CSS: &str = include_str!("../assets/chat.css");
#[cfg(feature = "mobile")]
const LONGPRESS_JS: &str = include_str!("../assets/longpress.js");

/// Wrapper so mpsc::Sender can be used as Dioxus context (needs Clone + Copy for rsx closures).
#[derive(Clone, Copy)]
pub struct RenderChannel {
    inner: Signal<std::sync::mpsc::Sender<commands::RenderRequest>>,
}

impl RenderChannel {
    pub fn sender(&self) -> std::sync::mpsc::Sender<commands::RenderRequest> {
        self.inner.read().clone()
    }
}

/// Bump this signal to trigger a database re-init (e.g. after changing library path).
pub type DbGeneration = Signal<u64>;

#[component]
pub fn App() -> Element {
    // Load config and provide as context
    let config = use_context_provider(|| Signal::new(SyncConfig::load()));

    // DB reload trigger — bump to force re-init without restart
    let db_generation: DbGeneration = use_context_provider(|| Signal::new(0u64));

    // Provide global state to all components
    use_context_provider(|| Signal::new(PdfTabManager::default()));
    use_context_provider(|| {
        let cfg = config.read();
        Signal::new(ViewerToolState {
            annotation_color: cfg.default_annotation_color.clone(),
            ..Default::default()
        })
    });
    use_context_provider(|| Signal::new(LibraryState::default()));
    use_context_provider(|| Signal::new(ShowSettings(false)));
    // New-collection editing state: None = not editing, Some(None) = top-level, Some(Some(id)) = subcollection
    use_context_provider(|| Signal::new(None::<Option<i64>>));
    // Drag paper state: paper_id being dragged from library to sidebar collections/tags
    use_context_provider(|| Signal::new(DragPaper(None)));
    // Undo/redo stack for annotation operations
    use_context_provider(|| Signal::new(crate::state::undo::UndoStack::default()));

    // Spawn dedicated PDF render thread and provide channel as context
    use_context_provider(|| RenderChannel {
        inner: Signal::new(commands::spawn_render_thread()),
    });

    // Chat state and ACP agent
    let chat_state: Signal<ChatState> =
        use_context_provider(|| Signal::new(ChatState::default()));
    let agent_channel: AgentChannel = use_context_provider(|| {
        let (req_tx, evt_rx) = crate::agent::spawn_agent_thread();
        // Store the event receiver in a signal so use_future can consume it
        // We use a OnceCell pattern to move the receiver into the future
        let rx_cell: Signal<Option<tokio::sync::mpsc::UnboundedReceiver<ChatEvent>>> =
            Signal::new(Some(evt_rx));
        // Spawn event polling future
        spawn_chat_event_poller(chat_state, rx_cell);
        AgentChannel {
            inner: Signal::new(Some(req_tx)),
        }
    });
    // Ensure agent_channel is provided as context (use_context_provider returns the value)
    let _ = agent_channel;

    // Initialize database asynchronously, re-runs when generation changes
    let db_gen = *db_generation.read();
    let db_resource = use_resource(move || async move {
        let _ = db_gen; // capture to re-run when generation bumps
        Database::init().await
    });

    match &*db_resource.read() {
        Some(Ok(db)) => {
            use_context_provider({
                let db = db.clone();
                move || db.clone()
            });

            rsx! {
                document::Style { {FONTS_CSS} }
                document::Style { {TOKENS_CSS} }
                document::Style { {BASE_CSS} }
                document::Style { {BUTTONS_CSS} }
                document::Style { {INPUTS_CSS} }
                document::Style { {LAYOUT_CSS} }
                document::Style { {SIDEBAR_CSS} }
                document::Style { {LIBRARY_CSS} }
                document::Style { {DETAIL_CSS} }
                document::Style { {PDF_CSS} }
                document::Style { {COMPONENTS_CSS} }
                document::Style { {DIALOGS_CSS} }
                document::Style { {THEME_CSS} }
                document::Style { {CHAT_CSS} }
                {longpress_script()}
                LoadLibraryData {}
                Layout {}
            }
        }
        Some(Err(e)) => {
            let err = e.clone();
            rsx! {
                document::Style { {FONTS_CSS} }
                document::Style { {TOKENS_CSS} }
                document::Style { {BASE_CSS} }
                document::Style { {LAYOUT_CSS} }
                document::Style { {THEME_CSS} }
                div { class: "db-error",
                    h1 { "Database Error" }
                    p { "{err}" }
                }
            }
        }
        None => {
            rsx! {
                document::Style { {FONTS_CSS} }
                document::Style { {TOKENS_CSS} }
                document::Style { {BASE_CSS} }
                document::Style { {LAYOUT_CSS} }
                document::Style { {THEME_CSS} }
                div { class: "db-error",
                    p { "Initializing database..." }
                }
            }
        }
    }
}

/// Spawns a Dioxus async future that polls the ACP event channel and updates ChatState.
fn spawn_chat_event_poller(
    mut chat_state: Signal<ChatState>,
    mut rx_cell: Signal<Option<tokio::sync::mpsc::UnboundedReceiver<ChatEvent>>>,
) {
    spawn(async move {
        // Take the receiver out of the signal (consumed once)
        let Some(mut rx) = rx_cell.write().take() else {
            return;
        };

        loop {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;

            // Drain all pending events this tick
            loop {
                let event = match rx.try_recv() {
                    Ok(event) => event,
                    Err(_) => break,
                };
                match event {
                ChatEvent::Connected { auth_methods, provider_id } => {
                    chat_state.with_mut(|s| {
                        s.messages.clear();
                        s.commands.clear();
                        s.session_active = false;
                        s.status = AgentStatus::Connecting;
                        s.auth_methods = auth_methods;
                        s.active_provider_id = provider_id;
                    });
                }
                ChatEvent::SessionCreated => {
                    chat_state.with_mut(|s| {
                        s.status = AgentStatus::Idle;
                        s.session_active = true;
                    });
                }
                ChatEvent::TextDelta(text) => {
                    chat_state.with_mut(|s| {
                        s.status = AgentStatus::Streaming;
                        // Append to last assistant message, or create one
                        if let Some(last) = s.messages.last_mut() {
                            if last.role == ChatRole::Assistant {
                                // Append to existing text content
                                if let Some(MessageContent::Text(t)) =
                                    last.content.last_mut()
                                {
                                    t.push_str(&text);
                                } else {
                                    last.content
                                        .push(MessageContent::Text(text));
                                }
                                return;
                            }
                        }
                        // Create new assistant message
                        s.messages.push(ChatMessage {
                            role: ChatRole::Assistant,
                            content: vec![MessageContent::Text(text)],
                            timestamp: chrono::Utc::now(),
                        });
                    });
                }
                ChatEvent::ToolCallStarted { id, title } => {
                    chat_state.with_mut(|s| {
                        s.status = AgentStatus::ToolCall(title.clone());
                        // Ensure there's an assistant message to attach the tool call to
                        if s.messages.last().map(|m| &m.role) != Some(&ChatRole::Assistant) {
                            s.messages.push(ChatMessage {
                                role: ChatRole::Assistant,
                                content: vec![],
                                timestamp: chrono::Utc::now(),
                            });
                        }
                        if let Some(last) = s.messages.last_mut() {
                            last.content.push(MessageContent::ToolUse {
                                id,
                                title,
                                status: crate::agent::types::ToolStatus::InProgress,
                            });
                        }
                    });
                }
                ChatEvent::ToolCallUpdated { id, status } => {
                    chat_state.with_mut(|s| {
                        // Find and update the tool call in the last assistant message
                        if let Some(last) = s.messages.last_mut() {
                            for content in &mut last.content {
                                if let MessageContent::ToolUse {
                                    id: tool_id,
                                    status: tool_status,
                                    ..
                                } = content
                                {
                                    if *tool_id == id {
                                        *tool_status = status.clone();
                                        break;
                                    }
                                }
                            }
                        }
                    });
                }
                ChatEvent::TurnCompleted => {
                    chat_state.with_mut(|s| s.status = AgentStatus::Idle);
                }
                ChatEvent::CommandsAvailable(commands) => {
                    chat_state.with_mut(|s| s.commands = commands);
                }
                ChatEvent::SessionList(sessions) => {
                    chat_state.with_mut(|s| {
                        s.past_sessions = sessions;
                        s.show_session_browser = true;
                    });
                }
                ChatEvent::Error(err) => {
                    chat_state.with_mut(|s| {
                        s.status = AgentStatus::Error(err.clone());
                        s.messages.push(ChatMessage {
                            role: ChatRole::Assistant,
                            content: vec![MessageContent::Error(err)],
                            timestamp: chrono::Utc::now(),
                        });
                    });
                }
            }
            } // inner drain loop
        } // outer poll loop
    });
}

#[cfg(feature = "mobile")]
fn longpress_script() -> Element {
    rsx! { document::Script { {LONGPRESS_JS} } }
}

#[cfg(not(feature = "mobile"))]
fn longpress_script() -> Element {
    rsx! {}
}

/// Loads library data from DB into signals on startup and when DB reloads.
#[component]
fn LoadLibraryData() -> Element {
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let db = use_context::<Database>();

    // Initial load
    let db2 = db.clone();
    use_effect(move || {
        let db = db2.clone();
        spawn(async move {
            // Check for external modifications (e.g. synced from another device)
            let db_path = db.data_dir().join("rotero.db");
            if crate::sync::engine::check_external_modification(&db_path, None) {
                eprintln!("Database was modified externally, reloading...");
            }

            let conn = db.conn();
            if let Ok(papers) = crate::db::papers::list_papers(conn).await {
                lib_state.with_mut(|s| s.papers = papers);
            }
            if let Ok(collections) = crate::db::collections::list_collections(conn).await {
                lib_state.with_mut(|s| s.collections = collections);
            }
            if let Ok(tags) = crate::db::tags::list_tags(conn).await {
                lib_state.with_mut(|s| s.tags = tags);
            }
            if let Ok(searches) = crate::db::saved_searches::list_saved_searches(conn).await {
                lib_state.with_mut(|s| s.saved_searches = searches);
            }

            // Store current modification time for future checks
            let _ = crate::sync::engine::file_modified_time(&db_path);
        });
    });

    // Poll the connector dirty flag to refresh after browser extension saves
    #[cfg(feature = "desktop")]
    use_future(move || {
        let db = db.clone();
        async move {
            use crate::state::app_state::LibraryView;
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                if let Some(flag) = crate::CONNECTOR_DIRTY.get()
                    && flag.swap(false, std::sync::atomic::Ordering::AcqRel)
                {
                    let conn = db.conn();
                    if let Ok(papers) = crate::db::papers::list_papers(conn).await {
                        lib_state.with_mut(|s| s.papers = papers);
                    }
                    // Refresh collection/tag paper IDs if viewing one
                    let view = lib_state.read().view.clone();
                    match view {
                        LibraryView::Collection(coll_id) => {
                            if let Ok(ids) =
                                crate::db::collections::list_paper_ids_in_collection(conn, coll_id)
                                    .await
                            {
                                lib_state.with_mut(|s| s.collection_paper_ids = Some(ids));
                            }
                        }
                        LibraryView::Tag(tag_id) => {
                            if let Ok(ids) =
                                crate::db::tags::list_paper_ids_by_tag(conn, tag_id).await
                            {
                                lib_state.with_mut(|s| s.tag_paper_ids = Some(ids));
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    });

    rsx! {}
}
