use dioxus::prelude::*;

#[component]
pub fn PdfViewer() -> Element {
    rsx! {
        div { class: "pdf-viewer",
            p { "PDF viewer — Phase 1" }
        }
    }
}
