use dioxus::prelude::*;

#[component]
pub fn CitationDialog() -> Element {
    rsx! {
        div { class: "citation-dialog",
            p { "Citation dialog — Phase 5" }
        }
    }
}
