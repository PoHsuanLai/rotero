use dioxus::prelude::*;

use crate::db::Database;
use crate::state::app_state::{LibraryState, PdfViewState};
use crate::state::commands;
use crate::sync::engine::SyncConfig;
use crate::ui::layout::Layout;

const FONTS_CSS: &str = include_str!("../assets/fonts.css");
const STYLE_CSS: &str = include_str!("../assets/style.css");

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

    // Provide global state to all components, using config defaults
    use_context_provider(|| {
        let cfg = config.read();
        Signal::new(PdfViewState {
            zoom: cfg.default_zoom,
            render_zoom: cfg.default_zoom,
            annotation_color: cfg.default_annotation_color.clone(),
            page_batch_size: Some(cfg.page_batch_size),
            ..Default::default()
        })
    });
    use_context_provider(|| Signal::new(LibraryState::default()));

    // Spawn dedicated PDF render thread and provide channel as context
    use_context_provider(|| RenderChannel {
        inner: Signal::new(commands::spawn_render_thread()),
    });

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
                document::Style { {STYLE_CSS} }
                LoadLibraryData {}
                Layout {}
            }
        }
        Some(Err(e)) => {
            let err = e.clone();
            rsx! {
                document::Style { {FONTS_CSS} }
                document::Style { {STYLE_CSS} }
                div { class: "db-error",
                    h1 { "Database Error" }
                    p { "{err}" }
                }
            }
        }
        None => {
            rsx! {
                document::Style { {FONTS_CSS} }
                document::Style { {STYLE_CSS} }
                div { class: "db-error",
                    p { "Initializing database..." }
                }
            }
        }
    }
}

/// Loads library data from DB into signals on startup and when DB reloads.
#[component]
fn LoadLibraryData() -> Element {
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let db = use_context::<Database>();

    use_effect(move || {
        let db = db.clone();
        spawn(async move {
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
        });
    });

    rsx! {}
}
