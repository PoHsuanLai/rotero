use dioxus::prelude::*;

#[component]
pub fn Settings() -> Element {
    rsx! {
        div { class: "settings",
            p { "Settings — Phase 2" }
        }
    }
}
