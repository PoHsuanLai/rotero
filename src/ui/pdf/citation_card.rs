//! Floating citation preview card.
//!
//! Clicking a citation link opens this card anchored at the click instead of
//! navigating away, so the reader can see the cited work in place. Internal
//! links parse the extracted reference into title / authors / year and then
//! try to resolve it to a real paper via search; external links (DOI/arXiv)
//! resolve against the library or fetch metadata from the web. In every case the
//! card offers the relevant action — Open (in library), Import (from the web),
//! or Jump to the target — and dismisses on click-away or Esc.

use dioxus::prelude::*;

use rotero_db::Database;
use rotero_models::{Paper, PaperId, normalize_title};

use super::PdfScrollLock;
use crate::app::{DevicePixelRatio, PdfDocs};
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
    let docs = use_context::<PdfDocs>();
    let import_channel = use_context::<ImportChannel>();
    let mut scroll_lock = use_context::<PdfScrollLock>();

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
                        if text.is_empty() {
                            ref_text.set(Some(String::new()));
                            resolved.set(Some(Vec::new()));
                        } else {
                            ref_text.set(Some(text.clone()));
                            let parsed = rotero_pdf::parse_reference(&text);
                            resolved.set(Some(resolve_citation(&db, &parsed).await));
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
                        let paper = match PaperId::from_url(&uri) {
                            Some(id) => fetch_web_by_id(&id).await,
                            None => None,
                        };
                        resolved.set(Some(paper.map(CardPaper::Web).into_iter().collect()));
                    }
                }
            });
        });
    }

    let is_internal = matches!(link, LinkDest::Internal { .. });

    // Jump-to: lock the viewer's scroll handler first. A smooth scroll to a
    // far page (e.g. p.1 → references on p.31) fires `onscroll` while the
    // viewport is still on p.1, which recenters the render window and cancels
    // the jump. Instant-scroll + dioxus.send waits until we've actually moved.
    let jump = {
        let link = link.clone();
        move || {
            if let LinkDest::Internal { page, y_frac } = link {
                let docs = docs.get();
                let data_dir = config.read().effective_library_path();
                scroll_lock.0.set(true);
                spawn(async move {
                    let mut eval = document::eval(&super::jump_to_page_js(page, y_frac));
                    let _ = eval.recv::<bool>().await;
                    crate::state::commands::ensure_window_rendered(
                        &docs, &mut tabs, tab_id, page, &data_dir,
                    )
                    .await;
                    scroll_lock.0.set(false);
                    on_close.call(());
                });
            }
        }
    };

    let resolved_list = resolved.read().clone();
    let has_resolved_papers = matches!(&resolved_list, Some(p) if !p.is_empty());
    let ref_snapshot = ref_text.read().clone();
    let parsed_ref = ref_snapshot
        .as_ref()
        .filter(|t| !t.is_empty())
        .map(|t| rotero_pdf::parse_reference(t));
    // Internal links already have extracted text to show; don't stack a
    // spinner under it. External links have only the URI until metadata lands.
    let show_resolving = resolved_list.is_none() && !(is_internal && ref_snapshot.is_some());

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

            // Extracted bibliography, shown until a library/web paper replaces it.
            if is_internal && !has_resolved_papers {
                match ref_snapshot.as_ref() {
                    None => rsx! {
                        div { class: "citation-card-loading", "Reading reference…" }
                    },
                    Some(text) if text.is_empty() => rsx! {
                        div { class: "citation-card-empty", "No reference text found at the target." }
                    },
                    Some(_) => rsx! {
                        if let Some(parsed) = parsed_ref {
                            if parsed.has_structure() {
                                {citation_bib_fields(
                                    parsed.title.clone().unwrap_or_default(),
                                    parsed.authors.clone().unwrap_or_default(),
                                    parsed.year.clone().unwrap_or_default(),
                                    parsed.venue.clone().unwrap_or_default(),
                                )}
                            } else {
                                div { class: "citation-card-reftext", "{parsed.text}" }
                            }
                        }
                    },
                }
            }

            // External links: show the linked URI immediately as a header, so the
            // card always has visible content while metadata resolves.
            if !is_internal {
                if let Some(uri) = external_uri.read().as_ref() {
                    div { class: "citation-card-uri",
                        i { class: "bi bi-link-45deg" }
                        span { class: "citation-card-uri-text", "{uri}" }
                    }
                }
            }

            // Resolved paper(s), or a resolving/empty state.
            if show_resolving {
                div { class: "citation-card-loading",
                    i { class: "bi bi-arrow-repeat external-spinner" }
                    span { "Resolving…" }
                }
            }

            match resolved_list {
                None => rsx! {},
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
                                    class: "citation-card-result",
                                    div { class: "citation-card-title", "{title}" }
                                    div { class: "citation-card-meta",
                                        span { class: "citation-card-authors", "{authors}" }
                                        if !year.is_empty() {
                                            span { class: "citation-card-sep", "\u{00b7}" }
                                            span { class: "citation-card-year", "{year}" }
                                        }
                                        if !journal.is_empty() {
                                            span { class: "citation-card-sep", "\u{00b7}" }
                                            span { class: "citation-card-journal", "{journal}" }
                                        }
                                    }
                                    if has_abstract {
                                        div { class: "citation-card-abstract", "{abstract_text}" }
                                    }
                                    div { class: "citation-card-result-actions",
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
                        onclick: move |evt| {
                            evt.stop_propagation();
                            jump.clone()();
                        },
                        "Jump to"
                    }
                }
            }
        }
    }
}

/// Resolve an extracted bibliography line to at most one paper.
///
/// Identifier (DOI / arXiv) uses the same lookup as an external PDF link.
/// Otherwise we search by parsed title and keep a hit only if the titles
/// actually overlap — a full-text search of the raw blob was returning three
/// loosely related OpenAlex results for every click.
async fn resolve_citation(db: &Database, parsed: &rotero_pdf::ParsedReference) -> Vec<CardPaper> {
    if let Some(uri) = identifier_in_reference(&parsed.text) {
        if let Some(paper) = db.find_paper_by_link(&uri).await.ok().flatten() {
            return vec![CardPaper::InLibrary(paper)];
        }
        if let Some(id) = PaperId::from_url(&uri).or_else(|| PaperId::parse(&uri))
            && let Some(paper) = fetch_web_by_id(&id).await
        {
            return vec![CardPaper::Web(paper)];
        }
    }

    let query = parsed.title.as_deref().unwrap_or("");
    if query.is_empty() {
        return Vec::new();
    }

    let local = db.search_papers(query).await.unwrap_or_default();
    if let Some(paper) = local.into_iter().find(|p| plausible_citation_hit(p, query)) {
        return vec![CardPaper::InLibrary(paper)];
    }

    if let Ok(paper) = rotero_search::openalex::search_by_title(query).await
        && plausible_citation_hit(&paper, query)
    {
        return vec![CardPaper::Web(paper)];
    }

    Vec::new()
}

async fn fetch_web_by_id(id: &PaperId) -> Option<Paper> {
    match id {
        PaperId::Doi(doi) => rotero_search::openalex::fetch_by_doi(doi)
            .await
            .or(rotero_search::crossref::fetch_by_doi(doi).await)
            .ok(),
        PaperId::ArXiv(aid) => rotero_search::arxiv::fetch_by_arxiv_id(aid).await.ok(),
        PaperId::Pmid(_) | PaperId::Isbn(_) => None,
    }
}

/// First DOI / arXiv / PMID token in a bibliography line, if any.
fn identifier_in_reference(text: &str) -> Option<String> {
    for word in text.split_whitespace() {
        let w =
            word.trim_matches(|c: char| matches!(c, ',' | ';' | '.' | ')' | '(' | '[' | ']' | '"'));
        let w = w
            .strip_prefix("doi:")
            .or_else(|| w.strip_prefix("DOI:"))
            .unwrap_or(w);
        if PaperId::from_url(w).is_some() || PaperId::parse(w).is_some() {
            return Some(w.to_string());
        }
    }
    None
}

fn plausible_citation_hit(paper: &Paper, query: &str) -> bool {
    let q = normalize_title(query);
    let nt = normalize_title(&paper.title);
    if q.is_empty() || nt.is_empty() {
        return false;
    }
    if nt == q || nt.contains(&q) || q.contains(&nt) {
        return true;
    }
    let q_tokens: Vec<&str> = q.split_whitespace().filter(|t| t.len() > 2).collect();
    if q_tokens.len() < 3 {
        return false;
    }
    let hits = q_tokens
        .iter()
        .filter(|t| nt.split_whitespace().any(|w| w == **t))
        .count();
    hits * 5 >= q_tokens.len() * 3
}

/// Title + authors/year/venue, shared by a parsed bibliography line and a
/// resolved library/web paper.
fn citation_bib_fields(title: String, authors: String, year: String, venue: String) -> Element {
    let has_meta = !authors.is_empty() || !year.is_empty() || !venue.is_empty();
    rsx! {
        div { class: "citation-card-result",
            if !title.is_empty() {
                div { class: "citation-card-title", "{title}" }
            }
            if has_meta {
                div { class: "citation-card-meta",
                    if !authors.is_empty() {
                        span { class: "citation-card-authors", "{authors}" }
                    }
                    if !year.is_empty() {
                        if !authors.is_empty() {
                            span { class: "citation-card-sep", "\u{00b7}" }
                        }
                        span { class: "citation-card-year", "{year}" }
                    }
                    if !venue.is_empty() {
                        if !authors.is_empty() || !year.is_empty() {
                            span { class: "citation-card-sep", "\u{00b7}" }
                        }
                        span { class: "citation-card-journal", "{venue}" }
                    }
                }
            }
        }
    }
}
