use dioxus::prelude::*;

use crate::state::app_state::LibraryState;
use crate::state::commands::ImportChannel;

use super::DetailShell;
use super::detail_fields::DetailFields;
use super::title_field::TitleField;

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
    // same non-empty DOI. Compared in canonical form, since that is what is
    // stored — a raw comparison never matched an arXiv paper found through a
    // provider that reports `10.48550/arXiv.X` for a stored `arXiv:X`.
    let already_imported = paper
        .canonical_doi()
        .filter(|doi| !doi.is_empty())
        .is_some_and(|doi| {
            state
                .papers
                .iter()
                .any(|p| p.canonical_doi().as_deref() == Some(doi.as_str()))
        });
    drop(state);

    // Resolve the identifier to its correct site (doi.org for real DOIs,
    // arxiv.org for arXiv, …) and label the button accordingly.
    let open_url = paper.resolve_url();
    let open_label = if paper.paper_id().is_some_and(|p| p.is_arxiv()) {
        "Open on arXiv"
    } else {
        "Open DOI"
    };
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

            TitleField { title: paper.title.clone(), item_type: paper.item_type.clone() }

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

                    if let Some(url) = open_url {
                        button {
                            class: "btn btn--secondary",
                            onclick: move |_| {
                                let _ = open::that(&url);
                            },
                            "{open_label}"
                        }
                    }
                }
            }
        }
    }
}
