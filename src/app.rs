use dioxus::prelude::*;

use rotero_db::Database;
use crate::state::app_state::{DragPaper, LibraryState, PdfTabManager, ViewerToolState};
use crate::state::commands;
use crate::sync::engine::SyncConfig;
use crate::ui::layout::Layout;

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

    // Initialize database asynchronously, re-runs when generation changes
    let db_gen = *db_generation.read();
    let db_resource = use_resource(move || async move {
        let _ = db_gen; // capture to re-run when generation bumps
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
                                        if let Some(p) =
                                            s.papers.iter_mut().find(|p| p.id.as_ref().map(|x| x.to_string()) == Some(paper_id.clone()))
                                        {
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
                                if let Some(p) = s.papers.iter_mut().find(|p| p.id.as_ref().map(|x| x.to_string()) == Some(pid.clone())) {
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
                    if let Ok(papers) = rotero_db::papers::list_papers(conn).await {
                        lib_state.with_mut(|s| s.papers = papers);
                    }
                    // Refresh collection/tag paper IDs if viewing one
                    let view = lib_state.read().view.clone();
                    match view {
                        LibraryView::Collection(coll_id) => {
                            if let Ok(ids) =
                                rotero_db::collections::list_paper_ids_in_collection(conn, &coll_id)
                                    .await
                            {
                                lib_state.with_mut(|s| s.collection_paper_ids = Some(ids));
                            }
                        }
                        LibraryView::Tag(tag_id) => {
                            if let Ok(ids) =
                                rotero_db::tags::list_paper_ids_by_tag(conn, &tag_id).await
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
