use dioxus::prelude::*;

use super::sidebar::Sidebar;
use super::library_view::LibraryPanel;
use super::pdf_viewer::PdfViewer;
use super::paper_detail::PaperDetail;
use crate::state::app_state::{LibraryState, LibraryView};
use crate::sync::engine::SyncConfig;

#[component]
pub fn Layout() -> Element {
    let lib_state = use_context::<Signal<LibraryState>>();
    let config = use_context::<Signal<SyncConfig>>();
    let view = lib_state.read().view.clone();

    let dark = config.read().dark_mode;
    let scale = config.read().ui_scale.clone();

    let container_class = if dark { "app-container dark" } else { "app-container" };

    rsx! {
        div {
            class: "{container_class}",
            "data-scale": "{scale}",
            Sidebar {}
            div { class: "main-panel",
                match view {
                    LibraryView::PdfViewer => rsx! { PdfViewer {} },
                    _ => rsx! {
                        div { style: "flex: 1; display: flex;",
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
