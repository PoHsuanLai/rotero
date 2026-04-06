use dioxus::prelude::*;

use crate::agent::types::{AgentStatus, ChatEvent, ChatMessage, ChatRole, ChatState, MessageContent};
use crate::state::app_state::{DragPaper, LibraryState, PdfTabManager, ViewerToolState};
use crate::state::commands;
use crate::sync::engine::SyncConfig;
use crate::ui::chat_panel::AgentChannel;
use crate::ui::layout::Layout;
use rotero_db::Database;

/// Global signal wrapper for settings dialog visibility.
#[derive(Debug, Clone, Copy, Default)]
pub struct ShowSettings(pub bool);

/// Device pixel ratio for HiDPI rendering.
#[derive(Debug, Clone, Copy)]
pub struct DevicePixelRatio(pub f32);

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
const GRAPH_CSS: &str = include_str!("../assets/graph.css");
const GRAPH_JS: &str = include_str!("../assets/graph.js");
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
    use_context_provider(|| Signal::new(None::<Option<String>>));
    // Drag paper state: paper_id being dragged from library to sidebar collections/tags
    use_context_provider(|| Signal::new(DragPaper(None)));
    // Undo/redo stack for annotation operations
    use_context_provider(|| Signal::new(crate::state::undo::UndoStack::default()));

    // Detect device pixel ratio for HiDPI rendering
    let mut dpr_signal = use_context_provider(|| Signal::new(DevicePixelRatio(1.0)));
    use_hook(move || {
        spawn(async move {
            let mut eval = document::eval("window.devicePixelRatio || 1.0");
            if let Ok(ratio) = eval.recv::<f64>().await {
                let ratio = (ratio as f32).max(1.0);
                dpr_signal.write().0 = ratio;
            }
        });
    });

    // Spawn dedicated PDF render thread and provide channel as context
    use_context_provider(|| RenderChannel {
        inner: Signal::new(commands::spawn_render_thread()),
    });

    // Chat state and ACP agent
    let mut chat_state: Signal<ChatState> =
        use_context_provider(|| Signal::new(ChatState::default()));
    let (agent_tx, agent_rx) = use_hook(|| {
        let (req_tx, evt_rx) = crate::agent::spawn_agent_thread();
        (
            Signal::new(Some(req_tx)),
            Signal::new(Some(evt_rx)),
        )
    });
    let agent_channel: AgentChannel = use_context_provider(|| AgentChannel { inner: agent_tx });
    let _ = agent_channel;

    // Poll agent events via use_future
    use_future(move || {
        let mut rx_sig = agent_rx;
        async move {
            let Some(mut rx) = rx_sig.write().take() else { return; };
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                while let Ok(event) = rx.try_recv() {
                    handle_chat_event(&mut chat_state, event);
                }
            }
        }
    });

    // Initialize database asynchronously, re-runs when generation changes
    let db_gen = *db_generation.read();
    let db_resource = use_resource(move || async move {
        let _ = db_gen; // capture to re-run when generation bumps
        #[cfg(feature = "desktop")]
        if let Some((conn, lib_path)) = crate::SHARED_DB.get() {
            return Ok(Database::from_conn(conn.clone(), lib_path.clone()));
        }
        let config = SyncConfig::load();
        Database::open(config.effective_library_path()).await
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
                document::Style { {GRAPH_CSS} }
                document::Style { {CHAT_CSS} }
                document::Script { {GRAPH_JS} }
                {longpress_script()}
                LoadLibraryData {}
                SyncLoop {}
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
    let config = use_context::<Signal<crate::sync::engine::SyncConfig>>();

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
            if let Ok(papers) = rotero_db::papers::list_papers(conn).await {
                lib_state.with_mut(|s| s.papers = papers);
            }
            if let Ok(collections) = rotero_db::collections::list_collections(conn).await {
                lib_state.with_mut(|s| s.collections = collections);
            }
            if let Ok(tags) = rotero_db::tags::list_tags(conn).await {
                lib_state.with_mut(|s| s.tags = tags);
            }
            if let Ok(searches) = rotero_db::saved_searches::list_saved_searches(conn).await {
                lib_state.with_mut(|s| s.saved_searches = searches);
            }

            // Store current modification time for future checks
            let _ = crate::sync::engine::file_modified_time(&db_path);
        });
    });

    // Background citation count refresh
    #[cfg(feature = "desktop")]
    {
        let db_cite = db.clone();
        use_future(move || {
            let db = db_cite.clone();
            async move {
                // Wait for initial library load to complete
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;

                loop {
                    // Find papers with DOI but no citation count — query DB directly
                    // to avoid cloning the entire papers Vec.
                    let needs_update = rotero_db::papers::list_papers_needing_citations(db.conn())
                        .await
                        .unwrap_or_default();

                    for (paper_id, doi) in needs_update {
                        let result = if doi.starts_with("arXiv:") {
                            let arxiv_id = doi.strip_prefix("arXiv:").unwrap_or(&doi);
                            crate::metadata::semantic_scholar::fetch_by_arxiv_id(arxiv_id).await
                        } else {
                            crate::metadata::semantic_scholar::fetch_by_doi(&doi).await
                        };

                        match result {
                            Ok(meta) => {
                                if let Some(count) = meta.citation_count {
                                    let _ = rotero_db::papers::update_citation_count(
                                        db.conn(),
                                        &paper_id,
                                        count,
                                    )
                                    .await;
                                    lib_state.with_mut(|s| {
                                        if let Some(p) = s.papers.iter_mut().find(|p| {
                                            p.id.as_deref() == Some(paper_id.as_str())
                                        }) {
                                            p.citation_count = Some(count);
                                        }
                                    });
                                }
                                // Normal rate limit: 3 seconds between requests
                                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                            }
                            Err(e) => {
                                if e.contains("429") {
                                    // Rate limited — back off for 60 seconds
                                    tracing::debug!("S2 rate limited, backing off 60s");
                                    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                                } else {
                                    tracing::debug!("Citation count fetch failed for {doi}: {e}");
                                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                                }
                            }
                        }
                    }

                    // Re-check every hour
                    tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
                }
            }
        });
    }

    // Background citation key generation + auto-export
    #[cfg(feature = "desktop")]
    {
        let db_bib = db.clone();
        use_future(move || {
            let db = db_bib.clone();
            async move {
                // Wait for initial load
                tokio::time::sleep(std::time::Duration::from_secs(4)).await;

                loop {
                    // Generate citation keys for papers that don't have one
                    let existing_keys = rotero_db::papers::list_citation_keys(db.conn())
                        .await
                        .unwrap_or_default();

                    // Query DB directly for papers needing keys — avoids cloning
                    // the entire papers Vec every 30 seconds.
                    let needs_keys =
                        rotero_db::papers::list_papers_needing_citation_keys(db.conn())
                            .await
                            .unwrap_or_default();
                    let mut keys_updated = false;
                    let mut all_keys = existing_keys;

                    for (paper_id, title, authors, year) in &needs_keys {
                        // Build a minimal Paper for key generation (only needs authors + year)
                        let mut stub = rotero_models::Paper::new(title.clone());
                        stub.id = Some(paper_id.clone());
                        stub.authors = authors.clone();
                        stub.year = *year;

                        let key = rotero_bib::generate_unique_cite_key(&stub, &all_keys);
                        if rotero_db::papers::update_citation_key(db.conn(), paper_id, &key)
                            .await
                            .is_ok()
                        {
                            let pid = paper_id.clone();
                            lib_state.with_mut(|s| {
                                if let Some(p) = s.papers.iter_mut().find(|p| {
                                    p.id.as_deref() == Some(pid.as_str())
                                }) {
                                    p.citation_key = Some(key.clone());
                                }
                            });
                            all_keys.push(key);
                            keys_updated = true;
                        }
                    }

                    // Auto-export .bib if configured and keys were updated
                    if keys_updated {
                        let config = config.read();
                        if let Some(ref bib_path) = config.auto_export_bib_path {
                            let state = lib_state.read();
                            let bib_content = rotero_bib::export_bibtex(&state.papers);
                            if let Err(e) = std::fs::write(bib_path, &bib_content) {
                                tracing::warn!("Auto-export .bib failed: {e}");
                            }
                        }
                    }

                    // Re-check every 30 seconds (quick for new imports)
                    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                }
            }
        });
    }

    // Await connector notifications to refresh after browser extension saves
    #[cfg(feature = "desktop")]
    use_future(move || {
        let db = db.clone();
        async move {
            use crate::state::app_state::LibraryView;
            // Clone the receiver out of the mutex so we can await it without holding the lock
            let mut rx = {
                let Some(lock) = crate::CONNECTOR_NOTIFY.get() else {
                    return;
                };
                let guard = lock.lock().unwrap();
                guard.clone()
            };
            loop {
                // Blocks until the connector actually sends a notification — zero CPU when idle
                if rx.changed().await.is_err() {
                    break; // sender dropped
                }
                let conn = db.conn();
                if let Ok(papers) = rotero_db::papers::list_papers(conn).await {
                    lib_state.with_mut(|s| s.papers = papers);
                }
                let view = lib_state.read().view.clone();
                match view {
                    LibraryView::Collection(coll_id) => {
                        if let Ok(ids) =
                            rotero_db::collections::list_paper_ids_in_collection(conn, &coll_id)
                                .await
                        {
                            lib_state.with_mut(|s| s.filter.collection_paper_ids = Some(ids));
                        }
                    }
                    LibraryView::Tag(tag_id) => {
                        if let Ok(ids) = rotero_db::tags::list_paper_ids_by_tag(conn, &tag_id).await
                        {
                            lib_state.with_mut(|s| s.filter.tag_paper_ids = Some(ids));
                        }
                    }
                    _ => {}
                }
            }
        }
    });

    rsx! {}
}

/// Background sync loop: periodically exports/imports changesets if sync is enabled.
#[component]
fn SyncLoop() -> Element {
    let db = use_context::<Database>();
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let config = use_context::<Signal<crate::sync::engine::SyncConfig>>();

    use_future(move || {
        let db = db.clone();
        async move {
            #[cfg(feature = "cloudkit")]
            let mut ck_engine: Option<crate::sync::cloudkit_sync::CloudKitSyncEngine> = None;

            loop {
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;

                let cfg = config.read().clone();
                if !cfg.sync_enabled {
                    continue;
                }

                let conn = db.conn();
                let site_id = match rotero_db::crr::site_id(conn).await {
                    Ok(id) => id,
                    Err(_) => continue,
                };

                let applied = match cfg.sync_transport {
                    crate::sync::engine::SyncTransport::File => {
                        let Some(ref folder) = cfg.sync_folder_path else {
                            continue;
                        };
                        let engine = crate::sync::file_sync::FileSyncEngine::new(
                            std::path::PathBuf::from(folder),
                            site_id,
                        );
                        if let Err(e) = engine.export_changes(conn).await {
                            tracing::warn!("File sync export failed: {e}");
                        }
                        let imported = match engine.import_changes(conn).await {
                            Ok(n) => n,
                            Err(e) => {
                                tracing::warn!("File sync import failed: {e}");
                                0
                            }
                        };
                        // Sync PDF files
                        let papers_dir = db.papers_dir();
                        let papers = lib_state.read().papers.clone();
                        for paper in &papers {
                            if let Some(ref path) = paper.pdf_path {
                                let _ = engine.export_pdf(&papers_dir, path);
                                let _ = engine.import_pdf(&papers_dir, path);
                            }
                        }
                        imported
                    }
                    crate::sync::engine::SyncTransport::CloudKit => {
                        #[cfg(feature = "cloudkit")]
                        {
                            let engine = ck_engine.get_or_insert_with(|| {
                                crate::sync::cloudkit_sync::CloudKitSyncEngine::new(site_id.clone())
                                    .expect("Failed to init CloudKit")
                            });
                            if let Err(e) = engine.export_changes(conn).await {
                                tracing::warn!("CloudKit export failed: {e}");
                            }
                            match engine.import_changes(conn).await {
                                Ok(n) => n,
                                Err(e) => {
                                    tracing::warn!("CloudKit import failed: {e}");
                                    0
                                }
                            }
                        }
                        #[cfg(not(feature = "cloudkit"))]
                        {
                            tracing::warn!("CloudKit sync selected but not compiled in");
                            0
                        }
                    }
                };

                if applied > 0 {
                    tracing::info!("Sync imported {applied} changes, refreshing library");
                    if let Ok(papers) = rotero_db::papers::list_papers(conn).await {
                        lib_state.with_mut(|s| s.papers = papers);
                    }
                    if let Ok(collections) = rotero_db::collections::list_collections(conn).await {
                        lib_state.with_mut(|s| s.collections = collections);
                    }
                    if let Ok(tags) = rotero_db::tags::list_tags(conn).await {
                        lib_state.with_mut(|s| s.tags = tags);
                    }
                }
            }
        }
    });

    rsx! {}
}

fn handle_chat_event(chat_state: &mut Signal<ChatState>, event: ChatEvent) {
    match event {
        ChatEvent::Switching { provider_id } => {
            chat_state.with_mut(|s| {
                s.messages.clear();
                s.commands.clear();
                s.session_active = false;
                s.auth_methods.clear();
                s.status = AgentStatus::Connecting;
                s.active_provider_id = provider_id;
            });
        }
        ChatEvent::Connected { auth_methods, provider_id, supports_list_sessions } => {
            chat_state.with_mut(|s| {
                s.status = AgentStatus::Connecting;
                s.auth_methods = auth_methods;
                s.active_provider_id = provider_id;
                s.supports_list_sessions = supports_list_sessions;
            });
        }
        ChatEvent::SessionCreated => {
            chat_state.with_mut(|s| {
                s.status = AgentStatus::Idle;
                s.session_active = true;
            });
        }
        ChatEvent::UserMessage(text) => {
            chat_state.with_mut(|s| {
                s.messages.push(ChatMessage {
                    role: ChatRole::User,
                    content: vec![MessageContent::Text(text)],
                    timestamp: chrono::Utc::now(),
                });
            });
        }
        ChatEvent::TextDelta(text) => {
            chat_state.with_mut(|s| {
                s.status = AgentStatus::Streaming;
                if let Some(last) = s.messages.last_mut() {
                    if last.role == ChatRole::Assistant {
                        if let Some(MessageContent::Text(t)) = last.content.last_mut() {
                            t.push_str(&text);
                        } else {
                            last.content.push(MessageContent::Text(text));
                        }
                        return;
                    }
                }
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
                        output: None,
                    });
                }
            });
        }
        ChatEvent::ToolCallUpdated { id, status, output } => {
            chat_state.with_mut(|s| {
                if let Some(last) = s.messages.last_mut() {
                    for content in &mut last.content {
                        if let MessageContent::ToolUse {
                            id: tool_id,
                            status: tool_status,
                            output: tool_output,
                            ..
                        } = content
                        {
                            if *tool_id == id {
                                *tool_status = status.clone();
                                if output.is_some() {
                                    *tool_output = output.clone();
                                }
                                break;
                            }
                        }
                    }
                }
            });
        }
        ChatEvent::TurnCompleted => {
            chat_state.with_mut(|s| {
                s.status = AgentStatus::Idle;
                // Mark any still-running tool calls as completed
                for msg in &mut s.messages {
                    for content in &mut msg.content {
                        if let MessageContent::ToolUse { status, .. } = content {
                            if matches!(status, crate::agent::types::ToolStatus::Pending | crate::agent::types::ToolStatus::InProgress) {
                                *status = crate::agent::types::ToolStatus::Completed;
                            }
                        }
                    }
                }
            });
        }
        ChatEvent::ModelsAvailable { models, current } => {
            chat_state.with_mut(|s| {
                s.available_models = models;
                s.current_model = current;
            });
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
        ChatEvent::AuthRequired { provider_name } => {
            chat_state.with_mut(|s| {
                s.status = AgentStatus::NeedsAuth;
                s.messages.push(ChatMessage {
                    role: ChatRole::Assistant,
                    content: vec![MessageContent::Text(
                        format!("Sign in to {provider_name} to get started. Go to Settings > AI Agent and use the Sign in option."),
                    )],
                    timestamp: chrono::Utc::now(),
                });
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
}
