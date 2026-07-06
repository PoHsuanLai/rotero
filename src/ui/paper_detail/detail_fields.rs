use dioxus::prelude::*;

use crate::ui::components::context_menu::{ContextMenu, ContextMenuItem};
use rotero_models::Paper;

/// The read-only bibliographic block shared by the library detail panel and the
/// web-result preview: title, authors, year, citations, journal, DOI (with a
/// copy / open-in-browser context menu), and abstract.
///
/// Fields that are missing on the paper are simply skipped, so the same
/// component renders a fully-populated library record and a sparse web hit
/// without either side special-casing.
#[component]
pub fn DetailFields(paper: Paper) -> Element {
    let authors_display = if paper.authors.is_empty() {
        "Unknown".to_string()
    } else {
        paper.authors.join(", ")
    };

    let mut doi_ctx = use_signal(|| None::<(String, f64, f64)>);

    rsx! {
        div { class: "detail-field",
            label { class: "detail-label", "Title" }
            div { class: "detail-value detail-value--title", "{paper.title}" }
        }

        div { class: "detail-field",
            label { class: "detail-label", "Authors" }
            div { class: "detail-value", "{authors_display}" }
        }

        if let Some(year) = paper.year {
            div { class: "detail-field",
                label { class: "detail-label", "Year" }
                div { class: "detail-value", "{year}" }
            }
        }

        if let Some(count) = paper.citation.citation_count {
            div { class: "detail-field",
                label { class: "detail-label", "Citations" }
                div { class: "detail-value detail-value--citations", "{count}" }
            }
        }

        if let Some(ref journal) = paper.publication.journal {
            div { class: "detail-field",
                label { class: "detail-label", "Journal" }
                div { class: "detail-value detail-value--journal", "{journal}" }
            }
        }

        if let Some(ref doi) = paper.doi {
            {
                let doi_for_ctx = doi.clone();
                rsx! {
                    div { class: "detail-field",
                        label { class: "detail-label", "DOI" }
                        div {
                            class: "detail-value detail-value--doi",
                            oncontextmenu: move |evt: Event<MouseData>| {
                                evt.prevent_default();
                                doi_ctx.set(Some((doi_for_ctx.clone(), evt.client_coordinates().x, evt.client_coordinates().y)));
                            },
                            "{doi}"
                        }
                    }
                }
            }
        }

        if let Some(ref abstract_text) = paper.abstract_text {
            div { class: "detail-field",
                label { class: "detail-label", "Abstract" }
                div {
                    class: "detail-value detail-value--abstract rendered-latex",
                    dangerous_inner_html: "{crate::ui::markdown::text_with_latex(abstract_text)}",
                }
            }
        }

        if let Some((doi_str, mx, my)) = doi_ctx() {
            {
                let doi_copy = doi_str.clone();
                let doi_open = doi_str.clone();
                rsx! {
                    ContextMenu {
                        x: mx,
                        y: my,
                        on_close: move |_| doi_ctx.set(None),

                        ContextMenuItem {
                            label: "Copy DOI".to_string(),
                            icon: Some("bi-clipboard".to_string()),
                            on_click: move |_| {
                                if let Ok(mut clip) = arboard::Clipboard::new() {
                                    let _ = clip.set_text(&*doi_copy);
                                }
                                doi_ctx.set(None);
                            },
                        }

                        ContextMenuItem {
                            label: "Open in browser".to_string(),
                            icon: Some("bi-box-arrow-up-right".to_string()),
                            on_click: move |_| {
                                // Route to the right resolver: doi.org rejects
                                // arXiv's `arXiv:ID` pseudo-DOI, so parse first.
                                let url = rotero_models::PaperId::parse(&doi_open)
                                    .map(|pid| pid.resolve_url())
                                    .unwrap_or_else(|| format!("https://doi.org/{doi_open}"));
                                let _ = open::that(&url);
                                doi_ctx.set(None);
                            },
                        }
                    }
                }
            }
        }
    }
}
