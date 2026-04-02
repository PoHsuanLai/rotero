use dioxus::prelude::*;

use super::sidebar::Sidebar;
use super::pdf_viewer::PdfViewer;
use crate::state::app_state::PdfViewState;

#[component]
pub fn Layout() -> Element {
    let pdf_state = use_context::<Signal<PdfViewState>>();
    let has_pdf = pdf_state.read().pdf_path.is_some();

    rsx! {
        div { class: "app-container",
            style: "display: flex; height: 100vh; font-family: system-ui, -apple-system, sans-serif;",
            Sidebar {}
            div { class: "main-panel",
                style: "flex: 1; display: flex; flex-direction: column; overflow: hidden;",
                if has_pdf {
                    PdfViewer {}
                } else {
                    WelcomeScreen {}
                }
            }
        }
    }
}

#[component]
fn WelcomeScreen() -> Element {
    rsx! {
        div {
            style: "flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; color: #666;",
            h1 { style: "font-size: 28px; font-weight: 300; margin-bottom: 8px;", "Welcome to Rotero" }
            p { style: "font-size: 16px; color: #999;", "Open a PDF from the sidebar to get started." }
        }
    }
}
