//! Floating citation preview card.
//!
//! Clicking a citation link opens this card anchored at the click instead of
//! navigating away, so the reader can see the cited work in place. Internal
//! links (in-text marker → references) show the extracted reference-block text
//! and try to resolve it to a real paper via search; external links (DOI/arXiv)
//! resolve against the library or fetch metadata from the web. In every case the
//! card offers the relevant action — Open (in library), Import (from the web),
//! or Jump to the target — and dismisses on click-away or Esc.

use dioxus::prelude::*;

use rotero_db::Database;
use rotero_models::{Paper, PaperId};

use crate::app::{DevicePixelRatio, RenderChannel};
use crate::state::app_state::{LibraryState, LinkDest, PdfTabManager, TabId};
use crate::state::commands::ImportChannel;
use crate::sync::engine::SyncConfig;

static CARD_ID: &str = "rotero-citation-card";

/// A paper surfaced in the card, tagged by where it came from so the action
/// buttons differ (Open for library papers, Import for web results).
#[derive(Clone)]
enum CardPaper {
    /// Already in the library.
    InLibrary(Paper),
    /// Found on the web; not yet imported.
    Web(Paper),
}

#[component]
pub(crate) fn CitationCard(
    /// Viewport x of the click (from `evt.client_coordinates()`).
    x: f64,
    /// Viewport y of the click.
    y: f64,
    link: LinkDest,
    tab_id: TabId,
    on_close: EventHandler<()>,
) -> Element {
    let db = use_context::<Database>();
    let mut tabs = use_context::<Signal<PdfTabManager>>();
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let config = use_context::<Signal<SyncConfig>>();
    let dpr_sig = use_context::<Signal<DevicePixelRatio>>();
    let render_ch = use_context::<RenderChannel>();
    let import_channel = use_context::<ImportChannel>();

    // Reference-block text for internal links (shown verbatim, always available).
    let mut ref_text = use_signal(|| None::<String>);
    // Resolved papers (library first, then web). `None` = still resolving.
    let mut resolved = use_signal(|| None::<Vec<CardPaper>>);
    // Raw URI to fall back to for external links that don't resolve.
    let mut external_uri = use_signal(|| None::<String>);
    // Which paper (by index) has been queued for import, for button state.
    let mut imported_idx = use_signal(|| None::<usize>);

    // Viewport-edge clamp (progressive enhancement — the card is already at
    // (x, y) from inline CSS). Mirrors the ContextMenu clamp.
    use_effect(move || {
        let js = format!(
            r#"requestAnimationFrame(() => {{
                let el = document.getElementById('{CARD_ID}');
                if (!el) return;
                let rect = el.getBoundingClientRect();
                let vw = window.innerWidth, vh = window.innerHeight;
                let nx = {x}, ny = {y};
                if (nx + rect.width > vw) nx = vw - rect.width - 8;
                if (ny + rect.height > vh) ny = vh - rect.height - 8;
                if (nx < 0) nx = 8;
                if (ny < 0) ny = 8;
                el.style.left = nx + 'px';
                el.style.top = ny + 'px';
            }})"#
        );
        spawn(async move {
            let _ = document::eval(&js).await;
        });
    });

    // Resolve the link once, on mount.
    {
        let db = db.clone();
        let link = link.clone();
        use_hook(move || {
            spawn(async move {
                match link {
                    LinkDest::Internal { page, y_frac } => {
                        // Pull the target page's cached text + pixel height, then
                        // extract the reference block starting at the link's y.
                        let (segments, page_height) = {
                            let mgr = tabs.read();
                            let tab = mgr.tabs.iter().find(|t| t.id == tab_id);
                            let seg = tab
                                .and_then(|t| t.render.text_data.get(&page))
                                .map(|td| td.segments.clone());
                            let h = tab
                                .and_then(|t| t.render.page_dims.get(page as usize))
                                .map(|(_, h)| *h as f64);
                            (seg, h)
                        };
                        let text = match (segments, page_height) {
                            (Some(segs), Some(h)) => {
                                let start_y = y_frac.unwrap_or(0.0) * h;
                                rotero_pdf::text_block_at(&segs, start_y, 8)
                            }
                            _ => String::new(),
                        };
                        if !text.is_empty() {
                            ref_text.set(Some(text.clone()));
                            // Resolve on demand: library FTS first, then the web.
                            let local = db.search_papers(&text).await.unwrap_or_default();
                            let mut out: Vec<CardPaper> = local
                                .into_iter()
                                .take(3)
                                .map(CardPaper::InLibrary)
                                .collect();
                            if out.is_empty()
                                && let Ok(web) =
                                    rotero_search::openalex::search_papers(&text, 3).await
                            {
                                out.extend(web.into_iter().map(CardPaper::Web));
                            }
                            resolved.set(Some(out));
                        } else {
                            ref_text.set(Some(String::new()));
                            resolved.set(Some(Vec::new()));
                        }
                    }
                    LinkDest::External { uri } => {
                        external_uri.set(Some(uri.clone()));
                        // Already in the library?
                        if let Some(paper) = db.find_paper_by_link(&uri).await.ok().flatten() {
                            resolved.set(Some(vec![CardPaper::InLibrary(paper)]));
                            return;
                        }
                        // Otherwise fetch metadata for a preview.
                        let fetched = match PaperId::from_url(&uri) {
                            Some(PaperId::Doi(doi)) => rotero_search::openalex::fetch_by_doi(&doi)
                                .await
                                .or(rotero_search::crossref::fetch_by_doi(&doi).await)
                                .ok(),
                            Some(PaperId::ArXiv(id)) => {
                                rotero_search::arxiv::fetch_by_arxiv_id(&id).await.ok()
                            }
                            _ => None,
                        };
                        resolved.set(Some(fetched.map(CardPaper::Web).into_iter().collect()));
                    }
                }
            });
        });
    }

    let is_internal = matches!(link, LinkDest::Internal { .. });

    // Jump-to action: reuse the existing internal-link navigation.
    let jump = {
        let link = link.clone();
        move || {
            if let LinkDest::Internal { page, y_frac } = link {
                let render_tx = render_ch.sender();
                let data_dir = config.read().effective_library_path();
                spawn(async move {
                    crate::state::commands::ensure_window_rendered(
                        &render_tx, &mut tabs, tab_id, page, &data_dir,
                    )
                    .await;
                    let js = match y_frac {
                        Some(f) => super::scroll_to_page_at_js(page, f),
                        None => super::scroll_to_page_js(page, "start"),
                    };
                    let _ = document::eval(&js).await;
                });
            }
        }
    };

    let resolved_list = resolved.read().clone();

    rsx! {
        div {
            class: "citation-card-backdrop",
            onclick: move |_| on_close.call(()),
        }
        div {
            id: CARD_ID,
            class: "citation-card",
            style: "left: {x}px; top: {y}px;",
            tabindex: "-1",
            onmounted: move |evt| {
                spawn(async move {
                    let _ = evt.set_focus(true).await;
                });
            },
            onkeydown: move |evt| {
                if evt.key() == Key::Escape {
                    on_close.call(());
                }
            },

            // Reference text (internal links) shown immediately, verbatim.
            if is_internal {
                if let Some(text) = ref_text.read().as_ref() {
                    if text.is_empty() {
                        div { class: "citation-card-empty", "No reference text found at the target." }
                    } else {
                        div { class: "citation-card-reftext", "{text}" }
                    }
                } else {
                    div { class: "citation-card-loading", "Reading reference…" }
                }
            }

            // Resolved paper(s), or a resolving/empty state.
            match resolved_list {
                None => rsx! {
                    div { class: "citation-card-loading",
                        i { class: "bi bi-arrow-repeat external-spinner" }
                        span { "Resolving…" }
                    }
                },
                Some(papers) if papers.is_empty() => rsx! {
                    // Nothing resolved. For external links offer the browser
                    // fallback; internal links already show their ref text.
                    if let Some(uri) = external_uri.read().as_ref() {
                        div { class: "citation-card-actions",
                            button {
                                class: "btn btn--sm btn--secondary",
                                onclick: {
                                    let uri = uri.clone();
                                    move |_| { let _ = open::that(&uri); on_close.call(()); }
                                },
                                "Open in browser"
                            }
                        }
                    }
                },
                Some(papers) => rsx! {
                    for (i, cp) in papers.into_iter().enumerate() {
                        {
                            let paper = match &cp { CardPaper::InLibrary(p) | CardPaper::Web(p) => p.clone() };
                            let in_library = matches!(cp, CardPaper::InLibrary(_));
                            let title = paper.title.clone();
                            let authors = paper.formatted_authors();
                            let year = paper.year.map(|y| y.to_string()).unwrap_or_default();
                            let journal = paper.publication.journal.clone().unwrap_or_default();
                            let abstract_text = paper.abstract_text.clone().unwrap_or_default();
                            let has_abstract = !abstract_text.is_empty() && !is_internal;
                            let already_imported = imported_idx.read().as_ref() == Some(&i);
                            rsx! {
                                div {
                                    key: "cp-{i}",
                                    class: "library-card citation-card-result",
                                    div { class: "library-card-body",
                                        div { class: "library-card-title", "{title}" }
                                        div { class: "library-card-meta",
                                            span { class: "library-card-authors", "{authors}" }
                                            if !year.is_empty() {
                                                span { class: "library-card-sep", "\u{00b7}" }
                                                span { class: "library-card-year", "{year}" }
                                            }
                                            if !journal.is_empty() {
                                                span { class: "library-card-sep", "\u{00b7}" }
                                                span { class: "library-card-journal", "{journal}" }
                                            }
                                        }
                                        if has_abstract {
                                            div { class: "external-result-abstract", "{abstract_text}" }
                                        }
                                    }
                                    div { class: "library-card-actions",
                                        if in_library {
                                            if paper.links.pdf_path.is_some() {
                                                button {
                                                    class: "btn btn--sm btn--primary",
                                                    onclick: {
                                                        let paper = paper.clone();
                                                        let db = db.clone();
                                                        move |_| {
                                                            if let (Some(pid), Some(rel)) = (
                                                                paper.id.as_deref(),
                                                                paper.links.pdf_path.as_deref(),
                                                            ) {
                                                                crate::state::commands::open_paper_pdf(
                                                                    &db, &mut tabs, &mut lib_state,
                                                                    &config, &dpr_sig, pid, rel, &paper.title,
                                                                );
                                                            }
                                                            on_close.call(());
                                                        }
                                                    },
                                                    "Open"
                                                }
                                            }
                                        } else if already_imported {
                                            button {
                                                class: "btn btn--sm btn--ghost external-imported-btn",
                                                disabled: true,
                                                i { class: "bi bi-check-lg" }
                                                "Imported"
                                            }
                                        } else {
                                            button {
                                                class: "btn btn--sm btn--primary",
                                                onclick: {
                                                    let paper = paper.clone();
                                                    move |_| {
                                                        import_channel.import(paper.clone());
                                                        imported_idx.set(Some(i));
                                                    }
                                                },
                                                "Import"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
            }

            // Jump-to is always available for internal links.
            if is_internal {
                div { class: "citation-card-actions",
                    button {
                        class: "btn btn--sm btn--secondary",
                        onclick: move |_| { jump.clone()(); on_close.call(()); },
                        "Jump to"
                    }
                }
            }
        }
    }
}
