use dioxus::prelude::*;

use super::sidebar::Sidebar;
use super::library_view::LibraryPanel;
use super::pdf_viewer::PdfViewer;
use super::paper_detail::PaperDetail;
use crate::state::app_state::{LibraryState, LibraryView};

#[component]
pub fn Layout() -> Element {
    let lib_state = use_context::<Signal<LibraryState>>();
    let view = lib_state.read().view.clone();

    rsx! {
        div { class: "app-container",
            style: "display: flex; height: 100vh; font-family: system-ui, -apple-system, sans-serif;",
            Sidebar {}
            div { class: "main-panel",
                style: "flex: 1; display: flex; overflow: hidden;",
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
