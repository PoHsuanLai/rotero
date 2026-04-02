use dioxus::prelude::*;

use super::sidebar::Sidebar;
use super::library_view::LibraryView;

#[component]
pub fn Layout() -> Element {
    rsx! {
        div { class: "app-container",
            style: "display: flex; height: 100vh; font-family: system-ui, -apple-system, sans-serif;",
            Sidebar {}
            div { class: "main-panel",
                style: "flex: 1; display: flex; flex-direction: column; overflow: hidden;",
                LibraryView {}
            }
        }
    }
}
