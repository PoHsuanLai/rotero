use dioxus::prelude::*;

#[component]
pub fn CollectionTreeItem(name: String, depth: u32) -> Element {
    let indent = depth * 16;
    rsx! {
        div {
            class: "sidebar-collection-item",
            style: "padding-left: {indent}px;",
            "{name}"
        }
    }
}
