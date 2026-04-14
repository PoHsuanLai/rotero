use dioxus::prelude::*;

use crate::state::app_state::LibraryView;

#[component]
pub fn LibraryEmptyState(view: LibraryView, is_searching: bool) -> Element {
    rsx! {
        div { class: "library-empty",
            if is_searching {
                p { class: "library-empty-heading", "No results found" }
                p { class: "library-empty-sub", "Try a different search term." }
            } else if matches!(view, LibraryView::Collection(_)) {
                p { class: "library-empty-heading", "No papers in this collection" }
                p { class: "library-empty-sub", "Drag papers from the library to add them." }
            } else if matches!(view, LibraryView::Tag(_)) {
                p { class: "library-empty-heading", "No papers with this tag" }
                p { class: "library-empty-sub", "Drag papers onto a tag in the sidebar to assign them." }
            } else if matches!(view, LibraryView::Favorites) {
                p { class: "library-empty-heading", "No favorites" }
                p { class: "library-empty-sub", "Right-click a paper and select Favorite to add it here." }
            } else if matches!(view, LibraryView::Unread) {
                p { class: "library-empty-heading", "All caught up" }
                p { class: "library-empty-sub", "No unread papers." }
            } else if matches!(view, LibraryView::Duplicates) {
                p { class: "library-empty-heading", "No duplicates found" }
                p { class: "library-empty-sub", "Your library has no duplicate papers." }
            } else {
                p { class: "library-empty-heading", "No papers yet" }
                p { class: "library-empty-sub", "Use \"+ Add PDF\" or the browser connector to import papers." }
            }
        }
    }
}
