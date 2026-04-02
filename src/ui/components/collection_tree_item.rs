use dioxus::prelude::*;

#[component]
pub fn CollectionTreeItem(name: String, depth: u32) -> Element {
    let indent = depth * 16;
    rsx! {
        div {
            style: "padding: 4px 8px 4px {indent}px; cursor: pointer; font-size: 14px;",
            "{name}"
        }
    }
}
