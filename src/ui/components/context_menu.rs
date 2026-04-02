use dioxus::prelude::*;

#[component]
pub fn ContextMenu() -> Element {
    rsx! {
        div { class: "context-menu",
            p { "Context menu — Phase 3" }
        }
    }
}
