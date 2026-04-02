use dioxus::prelude::*;

#[component]
pub fn Sidebar() -> Element {
    rsx! {
        div { class: "sidebar",
            style: "width: 250px; background: #f5f5f5; border-right: 1px solid #ddd; padding: 16px; overflow-y: auto;",
            h2 { style: "margin: 0 0 16px 0; font-size: 18px;", "Rotero" }
            div { class: "collections",
                h3 { style: "font-size: 14px; color: #666; margin: 8px 0;", "Collections" }
                p { style: "color: #999; font-size: 13px;", "No collections yet" }
            }
            div { class: "tags",
                h3 { style: "font-size: 14px; color: #666; margin: 8px 0;", "Tags" }
                p { style: "color: #999; font-size: 13px;", "No tags yet" }
            }
        }
    }
}
