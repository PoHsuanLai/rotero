use dioxus::prelude::*;

#[component]
pub fn TagBadge(name: String, color: Option<String>) -> Element {
    let bg = color.unwrap_or_else(|| "#e0e0e0".to_string());
    rsx! {
        span {
            style: "display: inline-block; padding: 2px 8px; border-radius: 12px; font-size: 12px; background: {bg};",
            "{name}"
        }
    }
}
