mod chat_handler;
mod library_loader;
mod sync_loop;

use dioxus::prelude::*;

use crate::agent::types::ChatState;
use crate::state::app_state::{DragPaper, LibraryState, PdfTabManager, ViewerToolState};
use crate::state::commands;
use crate::sync::engine::SyncConfig;
use crate::ui::chat_panel::AgentChannel;
use crate::ui::layout::Layout;
use rotero_db::Database;

use chat_handler::handle_chat_event;
use library_loader::LoadLibraryData;
use sync_loop::SyncLoop;

/// Global signal wrapper for settings dialog visibility.
#[derive(Debug, Clone, Copy, Default)]
pub struct ShowSettings(pub bool);

/// Device pixel ratio for HiDPI rendering.
#[derive(Debug, Clone, Copy)]
pub struct DevicePixelRatio(pub f32);

const FONTS_CSS: &str = include_str!("../../assets/fonts.css");
const TOKENS_CSS: &str = include_str!("../../assets/tokens.css");
const BASE_CSS: &str = include_str!("../../assets/base.css");
const BUTTONS_CSS: &str = include_str!("../../assets/buttons.css");
const INPUTS_CSS: &str = include_str!("../../assets/inputs.css");
const LAYOUT_CSS: &str = include_str!("../../assets/layout.css");
const SIDEBAR_CSS: &str = include_str!("../../assets/sidebar.css");
const LIBRARY_CSS: &str = include_str!("../../assets/library.css");
const DETAIL_CSS: &str = include_str!("../../assets/detail.css");
const PDF_CSS: &str = include_str!("../../assets/pdf.css");
const COMPONENTS_CSS: &str = include_str!("../../assets/components.css");
const DIALOGS_CSS: &str = include_str!("../../assets/dialogs.css");
const THEME_CSS: &str = include_str!("../../assets/theme.css");
const GRAPH_CSS: &str = include_str!("../../assets/graph.css");
const GRAPH_JS: &str = include_str!("../../assets/graph.js");
const CHAT_CSS: &str = include_str!("../../assets/chat.css");
#[cfg(feature = "mobile")]
const LONGPRESS_JS: &str = include_str!("../../assets/longpress.js");

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
    use_context_provider(|| {
        let mut mgr = PdfTabManager::default();
        mgr.set_max_resident(config.read().max_resident_tabs);
        Signal::new(mgr)
    });
    use_context_provider(|| {
        let cfg = config.read();
        Signal::new(ViewerToolState {
            annotation_color: cfg.ui.default_annotation_color.clone(),
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
