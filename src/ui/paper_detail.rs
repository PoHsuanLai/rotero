use dioxus::prelude::*;

#[component]
pub fn PaperDetail() -> Element {
    rsx! {
        div { class: "paper-detail",
            p { "Paper detail view — Phase 2" }
        }
    }
}
