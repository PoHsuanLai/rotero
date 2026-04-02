use dioxus::prelude::*;

use crate::db::Database;
use crate::state::app_state::{LibraryState, PdfViewState};
use crate::ui::layout::Layout;

#[component]
pub fn App() -> Element {
    // Initialize database once
    let db_result = use_signal(|| Database::init());

    // Provide global state to all components
    use_context_provider(|| Signal::new(PdfViewState::new()));
    use_context_provider(|| Signal::new(LibraryState::default()));

    match db_result.read().as_ref() {
        Ok(db) => {
            use_context_provider({
                let db = db.clone();
                move || db.clone()
            });

            rsx! {
                LoadLibraryData {}
                Layout {}
            }
        }
        Err(e) => {
            let err = e.clone();
            rsx! {
                div { style: "padding: 40px; color: #c00;",
                    h1 { "Database Error" }
                    p { "{err}" }
                }
            }
        }
    }
}

/// Loads library data from DB into signals on startup.
#[component]
fn LoadLibraryData() -> Element {
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let db = use_context::<Database>();

    use_effect(move || {
        let db = db.clone();
        if let Ok(papers) = db.with_conn(|conn| crate::db::papers::list_papers(conn)) {
            lib_state.with_mut(|s| s.papers = papers);
        }
        if let Ok(collections) = db.with_conn(|conn| crate::db::collections::list_collections(conn)) {
            lib_state.with_mut(|s| s.collections = collections);
        }
        if let Ok(tags) = db.with_conn(|conn| crate::db::tags::list_tags(conn)) {
            lib_state.with_mut(|s| s.tags = tags);
        }
    });

    rsx! {}
}
