use dioxus::prelude::*;

#[component]
pub fn CollectionTreeItem(name: String, depth: u32) -> Element {
    let indent = depth * 16;
    rsx! {
        div {
            class: "coll-row coll-row--sidebar",
            style: "padding-left: {indent}px;",
            "{name}"
        }
    }
}
