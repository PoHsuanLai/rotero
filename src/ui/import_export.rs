use dioxus::prelude::*;

#[component]
pub fn ImportExportDialog() -> Element {
    rsx! {
        div { class: "import-export",
            p { "Import/Export — Phase 4" }
        }
    }
}
