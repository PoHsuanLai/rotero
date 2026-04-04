use dioxus::prelude::*;

#[cfg(feature = "desktop")]
use super::keybindings::GlobalKeyHandler;
use super::graph_view::GraphView;
use super::library_view::LibraryPanel;
use super::paper_detail::PaperDetail;
use super::pdf_viewer::{PdfTabBar, PdfViewer};
use super::sidebar::Sidebar;
use crate::state::app_state::{LibraryState, LibraryView, PdfTabManager};
use crate::sync::engine::SyncConfig;

#[component]
pub fn Layout() -> Element {
    let lib_state = use_context::<Signal<LibraryState>>();
    let tab_mgr = use_context::<Signal<PdfTabManager>>();
    let config = use_context::<Signal<SyncConfig>>();
    let mut sidebar_collapsed = use_signal(|| false);
    let view = lib_state.read().view.clone();

    let dark = config.read().dark_mode;
    let scale = config.read().ui_scale.clone();
    let has_tabs = tab_mgr.read().active_tab_id.is_some();

    let container_class = if dark {
        "app-container dark"
    } else {
        "app-container"
    };

    #[cfg(feature = "desktop")]
    let key_handler = rsx! { GlobalKeyHandler {} };
    #[cfg(not(feature = "desktop"))]
    let key_handler = rsx! {};

    rsx! {
        {key_handler}
        div {
            class: "{container_class}",
            "data-scale": "{scale}",
            Sidebar {
                collapsed: sidebar_collapsed(),
                on_toggle: move |_| sidebar_collapsed.toggle(),
            }
            div { class: "main-panel",
                match view {
                    LibraryView::PdfViewer if has_tabs => rsx! {
                        PdfTabBar {}
                        PdfViewer {}
                    },
                    LibraryView::Graph => rsx! {
                        GraphView {}
                    },
                    _ => rsx! {
                        div { style: "flex: 1; display: flex; min-height: 0;",
                            LibraryPanel {}
                            if lib_state.read().selected_paper_id.is_some() {
                                PaperDetail {}
                            }
                        }
                    },
                }
            }
        }
    }
}
