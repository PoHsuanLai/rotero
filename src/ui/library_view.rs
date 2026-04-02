use dioxus::prelude::*;

#[component]
pub fn LibraryView() -> Element {
    rsx! {
        div { class: "library-view",
            style: "flex: 1; padding: 16px; overflow-y: auto;",
            h2 { style: "margin: 0 0 16px 0;", "All Papers" }
            p { style: "color: #999;", "No papers in your library. Add a PDF or use the browser connector to get started." }
        }
    }
}
