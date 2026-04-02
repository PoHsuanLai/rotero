use dioxus::prelude::*;

#[component]
pub fn TagBadge(name: String, color: Option<String>) -> Element {
    let bg = color.unwrap_or_else(|| "#e0e0e0".to_string());
    rsx! {
        span {
            class: "tag-badge",
            style: "background: {bg};",
            "{name}"
        }
    }
}
