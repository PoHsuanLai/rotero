use dioxus::prelude::*;

use crate::state::app_state::LibraryState;
use crate::state::commands::ImportChannel;

use super::DetailShell;
use super::detail_fields::DetailFields;

/// Read-only overview of a web search result that hasn't been imported yet.
///
/// Shares the [`DetailFields`] bibliographic block with the library detail
/// panel, so a web hit and a library record present identically. Since the
/// paper isn't in the library, this offers only Import and Open-DOI — no tags,
/// notes, or collections, which require a stored record.
#[component]
pub fn WebPreview() -> Element {
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let import_channel = use_context::<ImportChannel>();

    let state = lib_state.read();
    let paper = match state.previewed_web.clone() {
        Some(p) => p,
        None => return rsx! {},
    };
    // A web hit counts as already-imported when a library paper carries the
    // same non-empty DOI.
    let already_imported = paper.doi.as_deref().is_some_and(|doi| {
        !doi.is_empty()
            && state
                .papers
                .iter()
                .any(|p| p.doi.as_deref() == Some(doi))
    });
    drop(state);

    let doi_open = paper.doi.clone();
    let paper_for_import = paper.clone();

    rsx! {
        DetailShell {
            div { class: "detail-header",
                h3 { class: "detail-heading", "Details" }
                button {
                    class: "detail-close",
                    onclick: move |_| {
                        lib_state.with_mut(|s| s.previewed_web = None);
                    },
                    "\u{00d7}"
                }
            }

            div { class: "detail-web-badge", "From the web" }

            DetailFields { paper: paper.clone() }

            div { class: "detail-delete-section",
                div { class: "detail-actions",
                    if already_imported {
                        button {
                            class: "btn btn--ghost",
                            disabled: true,
                            i { class: "bi bi-check-lg" }
                            " In library"
                        }
                    } else {
                        button {
                            class: "btn btn--primary",
                            onclick: move |_| {
                                import_channel.import(paper_for_import.clone());
                                // Reflect the import immediately: drop the preview
                                // so the panel returns to the library view.
                                lib_state.with_mut(|s| s.previewed_web = None);
                            },
                            "Import"
                        }
                    }

                    if let Some(doi) = doi_open {
                        if !doi.is_empty() {
                            button {
                                class: "btn btn--secondary",
                                onclick: move |_| {
                                    let url = format!("https://doi.org/{doi}");
                                    let _ = open::that(&url);
                                },
                                "Open DOI"
                            }
                        }
                    }
                }
            }
        }
    }
}
