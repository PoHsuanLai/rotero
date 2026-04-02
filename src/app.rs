use dioxus::prelude::*;

use crate::db::Database;
use crate::state::app_state::{LibraryState, PdfViewState};
use crate::ui::layout::Layout;

#[component]
pub fn App() -> Element {
    // Provide global state to all components
    use_context_provider(|| Signal::new(PdfViewState::new()));
    use_context_provider(|| Signal::new(LibraryState::default()));

    // Initialize database asynchronously
    let db_resource = use_resource(|| async { Database::init().await });

    match &*db_resource.read() {
        Some(Ok(db)) => {
            use_context_provider({
                let db = db.clone();
                move || db.clone()
            });

            rsx! {
                document::Link { rel: "stylesheet", href: asset!("/assets/style.css") }
                LoadLibraryData {}
                Layout {}
            }
        }
        Some(Err(e)) => {
            let err = e.clone();
            rsx! {
                document::Link { rel: "stylesheet", href: asset!("/assets/style.css") }
                div { class: "db-error",
                    h1 { "Database Error" }
                    p { "{err}" }
                }
            }
        }
        None => {
            rsx! {
                document::Link { rel: "stylesheet", href: asset!("/assets/style.css") }
                div { class: "db-error",
                    p { "Initializing database..." }
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
        spawn(async move {
            let conn = db.conn();
            if let Ok(papers) = crate::db::papers::list_papers(conn).await {
                lib_state.with_mut(|s| s.papers = papers);
            }
            if let Ok(collections) = crate::db::collections::list_collections(conn).await {
                lib_state.with_mut(|s| s.collections = collections);
            }
            if let Ok(tags) = crate::db::tags::list_tags(conn).await {
                lib_state.with_mut(|s| s.tags = tags);
            }
        });
    });

    rsx! {}
}
