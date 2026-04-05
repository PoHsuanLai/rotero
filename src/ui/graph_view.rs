use dioxus::prelude::*;
use rotero_graph::{GraphData, GraphFilter};

use crate::state::app_state::{LibraryState, LibraryView, PdfTabManager};
use rotero_db::Database;

/// Graph event sent from JS canvas via dioxus.send().
#[derive(serde::Deserialize)]
struct GraphEvent {
    #[serde(rename = "type")]
    event_type: String,
    id: String,
}

/// Which edge type to display — only one at a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EdgeMode {
    Tags,
    Collections,
    Authors,
    Journals,
}

impl EdgeMode {
    fn label(self) -> &'static str {
        match self {
            Self::Tags => "Tags",
            Self::Collections => "Collections",
            Self::Authors => "Authors",
            Self::Journals => "Journals",
        }
    }

    fn to_filter(self) -> GraphFilter {
        GraphFilter {
            show_tag_edges: self == Self::Tags,
            show_collection_edges: self == Self::Collections,
            show_author_edges: self == Self::Authors,
            show_journal_edges: self == Self::Journals,
            ..Default::default()
        }
    }

    fn edge_color(self) -> &'static str {
        match self {
            Self::Tags => "#0d9488",
            Self::Collections => "#6366f1",
            Self::Authors => "#f59e0b",
            Self::Journals => "#94a3b8",
        }
    }
}

const ALL_MODES: [EdgeMode; 4] = [
    EdgeMode::Tags,
    EdgeMode::Collections,
    EdgeMode::Authors,
    EdgeMode::Journals,
];

#[component]
pub fn GraphView() -> Element {
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let db = use_context::<Database>();
    let mut tabs = use_context::<Signal<PdfTabManager>>();
    let config = use_context::<Signal<crate::sync::engine::SyncConfig>>();
    let dpr = use_context::<Signal<crate::app::DevicePixelRatio>>();

    let mut graph_json = use_signal(|| None::<String>);
    let mut edge_mode = use_signal(|| EdgeMode::Tags);
    let mut search_query = use_signal(|| String::new());
    let mut initialized = use_signal(|| false);

    // Compute graph data when papers or edge mode changes
    let db2 = db.clone();
    use_effect(move || {
        let mode = edge_mode();
        let db = db2.clone();

        spawn(async move {
            let state = lib_state.read();
            let papers = state.papers.clone();
            let tags = state.tags.clone();
            drop(state);

            let conn = db.conn();
            let tag_pairs = rotero_db::graph::list_all_paper_tags(conn)
                .await
                .unwrap_or_default();
            let coll_pairs = rotero_db::graph::list_all_paper_collections(conn)
                .await
                .unwrap_or_default();

            let filter = mode.to_filter();

            let mut data = rotero_graph::build_and_simulate(
                &papers,
                &tags,
                &tag_pairs,
                &coll_pairs,
                &filter,
                200,
            );

            // Attach full metadata for tooltips
            for node in &mut data.nodes {
                if let Some(paper) = papers
                    .iter()
                    .find(|p| p.id.as_deref() == Some(node.id.as_str()))
                {
                    node.label = truncate(&paper.title, 25);
                }
            }

            let json = build_js_data(&data, &papers);
            graph_json.set(Some(json));
        });
    });

    // Initialize canvas and push data when ready
    use_effect(move || {
        if let Some(ref json) = *graph_json.read() {
            let json = json.clone();
            let is_init = initialized();
            spawn(async move {
                if !is_init {
                    let _ = document::eval(
                        "window.__roteroGraph.init('graph-canvas', 'graph-tooltip')",
                    );
                    initialized.set(true);
                }
                let escaped = json.replace('\\', "\\\\").replace('`', "\\`");
                let _ = document::eval(&format!("window.__roteroGraph.setData(`{escaped}`)"));
            });
        }
    });

    // Handle search highlighting
    use_effect(move || {
        let query = search_query().to_lowercase();
        let state = lib_state.read();
        if query.is_empty() {
            spawn(async move {
                let _ = document::eval("window.__roteroGraph.highlight(null)");
            });
        } else {
            let matching_ids: Vec<String> = state
                .papers
                .iter()
                .filter(|p| {
                    p.title.to_lowercase().contains(&query)
                        || p.authors.iter().any(|a| a.to_lowercase().contains(&query))
                })
                .filter_map(|p| p.id.clone())
                .collect();
            spawn(async move {
                let ids_json = serde_json::to_string(&matching_ids).unwrap_or_default();
                let _ = document::eval(&format!("window.__roteroGraph.highlight({ids_json})"));
            });
        }
    });

    // Listen for click events from JS — uses a never-resolving promise to keep the channel open
    use_hook(move || {
        spawn(async move {
            let mut eval = document::eval(
                // Keep eval alive without polling — waits forever so dioxus.send() can deliver messages
                "new Promise(() => {})",
            );
            loop {
                match eval.recv::<String>().await {
                    Ok(msg) => {
                        if let Ok(event) = serde_json::from_str::<GraphEvent>(&msg) {
                            match event.event_type.as_str() {
                                "click" => {
                                    lib_state.with_mut(|s| {
                                        s.selected_paper_id = Some(event.id.clone());
                                    });
                                }
                                "dblclick" => {
                                    let state = lib_state.read();
                                    if let Some(paper) = state
                                        .papers
                                        .iter()
                                        .find(|p| p.id.as_deref() == Some(event.id.as_str()))
                                    {
                                        if let Some(ref pdf_path) = paper.pdf_path {
                                            let abs_path = db
                                                .resolve_pdf_path(pdf_path)
                                                .to_string_lossy()
                                                .to_string();
                                            let title = paper.title.clone();
                                            let pid = event.id.clone();
                                            drop(state);
                                            let cfg = config.read();
                                            let dpr_val = dpr.read().0;
                                            tabs.with_mut(|m| {
                                                m.open_or_switch(
                                                    pid,
                                                    abs_path,
                                                    title,
                                                    cfg.default_zoom,
                                                    cfg.page_batch_size,
                                                    dpr_val,
                                                )
                                            });
                                            lib_state.with_mut(|s| s.view = LibraryView::PdfViewer);
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
        });
    });

    let paper_count = lib_state.read().papers.len();
    let current_mode = edge_mode();

    rsx! {
        div { class: "graph-view",
            // Toolbar
            div { class: "graph-toolbar",
                // Edge mode tabs
                div { class: "graph-mode-tabs",
                    for mode in ALL_MODES {
                        button {
                            class: if current_mode == mode { "graph-mode-tab active" } else { "graph-mode-tab" },
                            style: if current_mode == mode { format!("--tab-color: {}", mode.edge_color()) } else { String::new() },
                            onclick: move |_| edge_mode.set(mode),
                            "{mode.label()}"
                        }
                    }
                }

                div { class: "graph-toolbar-sep" }

                button {
                    title: "Re-center",
                    onclick: move |_| {
                        spawn(async move {
                            let _ = document::eval("window.__roteroGraph.recenter()");
                        });
                    },
                    i { class: "bi bi-arrows-fullscreen" }
                }

                input {
                    class: "graph-search",
                    r#type: "text",
                    placeholder: "Search papers...",
                    value: search_query(),
                    oninput: move |evt: Event<FormData>| {
                        search_query.set(evt.value());
                    },
                }
            }

            if paper_count == 0 {
                div { class: "graph-empty", "No papers in library" }
            } else {
                // Canvas container
                div { class: "graph-canvas-wrap",
                    canvas { id: "graph-canvas" }
                    div { id: "graph-tooltip", class: "graph-tooltip" }
                }
            }
        }
    }
}

/// Build JSON data string with full metadata for tooltips.
fn build_js_data(data: &GraphData, papers: &[rotero_models::Paper]) -> String {
    #[derive(serde::Serialize)]
    struct JsNode {
        id: String,
        label: String,
        x: f64,
        y: f64,
        size: f64,
        color: String,
        is_read: bool,
        is_favorite: bool,
        #[serde(rename = "_fullTitle")]
        full_title: String,
        #[serde(rename = "_authors")]
        authors: String,
        #[serde(rename = "_year")]
        year: Option<i32>,
    }

    let nodes: Vec<JsNode> = data
        .nodes
        .iter()
        .map(|n| {
            let paper = papers
                .iter()
                .find(|p| p.id.as_deref() == Some(n.id.as_str()));
            JsNode {
                id: n.id.clone(),
                label: n.label.clone(),
                x: n.x,
                y: n.y,
                size: n.size,
                color: n.color.clone(),
                is_read: n.is_read,
                is_favorite: n.is_favorite,
                full_title: paper.map(|p| p.title.clone()).unwrap_or_default(),
                authors: paper.map(|p| p.authors.join(", ")).unwrap_or_default(),
                year: paper.and_then(|p| p.year),
            }
        })
        .collect();

    #[derive(serde::Serialize)]
    struct JsData<'a> {
        nodes: Vec<JsNode>,
        links: &'a [rotero_graph::GraphEdge],
    }

    serde_json::to_string(&JsData {
        nodes,
        links: &data.links,
    })
    .unwrap_or_else(|_| "{}".to_string())
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut end = max - 3;
    while !s.is_char_boundary(end) && end > 0 {
        end -= 1;
    }
    format!("{}...", &s[..end])
}
