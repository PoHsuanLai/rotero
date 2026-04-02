use dioxus::prelude::*;

#[component]
pub fn SearchBar() -> Element {
    rsx! {
        div { class: "search-bar",
            input { r#type: "text", placeholder: "Search papers..." }
        }
    }
}
