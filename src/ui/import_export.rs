use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use dioxus::prelude::*;

use crate::state::app_state::LibraryState;
use rotero_db::Database;

#[derive(Clone, PartialEq)]
pub struct OaPending {
    pub id: String,
    pub doi: Option<String>,
    pub title: String,
    pub first_author: Option<String>,
    pub year: Option<i32>,
}

/// OA download flow state, persisted as context across view switches.
#[derive(Clone, PartialEq)]
pub enum OaState {
    Prompt(Vec<OaPending>),
    Downloading {
        done: usize,
        total: usize,
        downloaded: usize,
    },
    Done {
        downloaded: usize,
        total: usize,
    },
}

#[derive(Clone)]
pub struct OaCancelFlag(pub Signal<Option<Arc<AtomicBool>>>);

#[component]
pub fn ImportExportButtons() -> Element {
    rsx! {
        div { class: "import-export-row",
            ImportButton {}
            ExportBibtexButton {}
        }
    }
}

/// Must be rendered in a component that stays mounted across view switches (e.g. Layout).
#[component]
pub fn OaOverlay() -> Element {
    let oa_state = use_context::<Signal<Option<OaState>>>();
    let cancel_flag = use_context::<OaCancelFlag>();

    rsx! {
        if let Some(OaState::Prompt(ref papers)) = oa_state() {
            OaPromptDialog { papers: papers.clone() }
        }
        match oa_state() {
            Some(OaState::Downloading { done, total, downloaded }) => rsx! {
                OaProgressBanner { done, total, downloaded, cancel_flag: cancel_flag.0 }
            },
            Some(OaState::Done { downloaded, total }) => rsx! {
                OaDoneBanner { downloaded, total }
            },
            _ => rsx! {}
        }
    }
}

#[component]
fn ImportButton() -> Element {
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let db = use_context::<Database>();
    let mut oa_state = use_context::<Signal<Option<OaState>>>();
    let mut status = use_signal(|| None::<String>);

    rsx! {
        button {
            class: "btn btn--ghost btn--sm",
            onclick: move |_| {
                let db = db.clone();
                spawn(async move {
                    let file = super::pick_file_async(
                        &["bib", "bibtex", "ris", "json", "nbib"],
                        "Import Library",
                    ).await;
                    if let Some(path) = file {
                        match std::fs::read_to_string(&path) {
                            Ok(content) => {
                                let ext = path.extension()
                                    .and_then(|e| e.to_str())
                                    .unwrap_or("")
                                    .to_lowercase();

                                let bib_dir = path.parent().map(|p| p.to_path_buf());

                                let parsed: Result<Vec<(rotero_models::Paper, Option<String>)>, String> = match ext.as_str() {
                                    "ris" => rotero_bib::import_ris(&content)
                                        .map(|papers| papers.into_iter().map(|p| (p, None)).collect()),
                                    "json" => rotero_bib::import_csl_json(&content)
                                        .map(|papers| papers.into_iter().map(|p| (p, None)).collect()),
                                    "nbib" => rotero_bib::import_nbib(&content)
                                        .map(|papers| papers.into_iter().map(|p| (p, None)).collect()),
                                    _ => rotero_bib::import_bibtex(&content)
                                        .map(|entries| entries.into_iter().map(|e| (e.paper, e.source_pdf)).collect()),
                                };

                                match parsed {
                                    Ok(entries) => {
                                        let count = entries.len();
                                        let mut imported = 0;
                                        let mut pdfs_found = 0;
                                        let mut needs_oa = Vec::new();

                                        for (paper, source_pdf) in entries {
                                            if let Ok(id) = rotero_db::papers::insert_paper(db.conn(), &paper).await {
                                                let mut paper = paper;
                                                paper.id = Some(id.clone());

                                                if let (Some(bib_dir), Some(rel_pdf)) = (&bib_dir, &source_pdf) {
                                                    let pdf_abs = bib_dir.join(rel_pdf);
                                                    if pdf_abs.exists()
                                                        && let Ok(rel_path) = db.import_pdf(
                                                            pdf_abs.to_str().unwrap_or_default(),
                                                            Some(paper.title.as_str()),
                                                            paper.authors.first().map(|a| a.as_str()),
                                                            paper.year,
                                                        ) {
                                                            let _ = rotero_db::papers::update_pdf_path(db.conn(), &id, &rel_path).await;
                                                            paper.links.pdf_path = Some(rel_path);
                                                            pdfs_found += 1;
                                                        }
                                                }

                                                if paper.links.pdf_path.is_none() {
                                                    needs_oa.push(OaPending {
                                                        id,
                                                        doi: paper.doi.clone(),
                                                        title: paper.title.clone(),
                                                        first_author: paper.authors.first().cloned(),
                                                        year: paper.year,
                                                    });
                                                }

                                                lib_state.with_mut(|s| s.papers.insert(0, paper));
                                                imported += 1;
                                            }
                                        }

                                        let pdf_msg = if pdfs_found > 0 {
                                            format!(" ({pdfs_found} PDFs)")
                                        } else {
                                            String::new()
                                        };
                                        status.set(Some(format!("Imported {imported}/{count} papers{pdf_msg}")));

                                        if !needs_oa.is_empty() {
                                            oa_state.set(Some(OaState::Prompt(needs_oa)));
                                        }
                                    }
                                    Err(e) => status.set(Some(format!("Parse error: {e}"))),
                                }
                            }
                            Err(e) => status.set(Some(format!("Read error: {e}"))),
                        }
                    }
                });
            },
            i { class: "bi bi-download" }
            " Import"
        }
        if let Some(msg) = status.read().as_ref() {
            span { class: "import-status", "{msg}" }
        }
    }
}

#[component]
fn OaPromptDialog(papers: Vec<OaPending>) -> Element {
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let db = use_context::<Database>();
    let mut oa_state = use_context::<Signal<Option<OaState>>>();
    let cancel_ctx = use_context::<OaCancelFlag>();
    let mut cancel_flag = cancel_ctx.0;
    let count = papers.len();

    rsx! {
        div { class: "citation-overlay",
            onclick: move |_| oa_state.set(None),

            div { class: "citation-dialog oa-dialog",
                onclick: move |evt| evt.stop_propagation(),

                div { class: "oa-dialog-header",
                    h3 { "Download Open Access PDFs" }
                    button {
                        class: "detail-close",
                        onclick: move |_| oa_state.set(None),
                        "\u{00d7}"
                    }
                }
                p { class: "oa-dialog-text",
                    "{count} imported papers don't have PDFs. Search OpenAlex for open access versions?"
                }
                div { class: "oa-dialog-actions",
                    button {
                        class: "btn btn--ghost",
                        onclick: move |_| oa_state.set(None),
                        "Skip"
                    }
                    button {
                        class: "btn btn--primary",
                        onclick: move |_| {
                            let papers = papers.clone();
                            let total = papers.len();
                            let db = db.clone();
                            let cancelled = Arc::new(AtomicBool::new(false));
                            cancel_flag.set(Some(cancelled.clone()));
                            oa_state.set(Some(OaState::Downloading { done: 0, total, downloaded: 0 }));

                            spawn(async move {
                                let mut downloaded = 0;
                                for (i, p) in papers.iter().enumerate() {
                                    if cancelled.load(Ordering::Relaxed) {
                                        break;
                                    }
                                    oa_state.set(Some(OaState::Downloading { done: i + 1, total, downloaded }));
                                    let urls = crate::metadata::pdf_download::resolve_pdf_urls(p.doi.as_deref(), &p.title).await;
                                    if cancelled.load(Ordering::Relaxed) { break; }
                                    if let Ok(rel_path) = crate::metadata::pdf_download::download_and_save_pdf(
                                        &db, &urls, &p.title, p.first_author.as_deref(), p.year,
                                    ).await {
                                        let _ = rotero_db::papers::update_pdf_path(db.conn(), &p.id, &rel_path).await;
                                        let pid = p.id.clone();
                                        lib_state.with_mut(|s| {
                                            if let Some(paper) = s.papers.iter_mut().find(|paper| paper.id.as_deref() == Some(pid.as_str())) {
                                                paper.links.pdf_path = Some(rel_path);
                                            }
                                        });
                                        downloaded += 1;
                                    }
                                }
                                oa_state.set(Some(OaState::Done { downloaded, total }));
                            });
                        },
                        "Download"
                    }
                }
            }
        }
    }
}

#[component]
fn OaProgressBanner(
    done: usize,
    total: usize,
    downloaded: usize,
    cancel_flag: Signal<Option<Arc<AtomicBool>>>,
) -> Element {
    let mut oa_state = use_context::<Signal<Option<OaState>>>();
    let pct = if total > 0 {
        done as f64 / total as f64 * 100.0
    } else {
        0.0
    };

    rsx! {
        div { class: "oa-banner",
            div { class: "oa-banner-content",
                div { class: "oa-banner-text",
                    "Downloading PDFs... {done}/{total} ({downloaded} found)"
                }
                button {
                    class: "btn btn--ghost btn--xs",
                    onclick: move |_| {
                        if let Some(flag) = cancel_flag() {
                            flag.store(true, Ordering::Relaxed);
                        }
                        oa_state.set(None);
                    },
                    "Cancel"
                }
            }
            div { class: "oa-progress-bar",
                div {
                    class: "oa-progress-fill",
                    style: "width: {pct}%",
                }
            }
        }
    }
}

#[component]
fn OaDoneBanner(downloaded: usize, total: usize) -> Element {
    let mut oa_state = use_context::<Signal<Option<OaState>>>();

    rsx! {
        div { class: "oa-banner oa-banner--done",
            div { class: "oa-banner-content",
                div { class: "oa-banner-text",
                    "Found {downloaded} open access PDFs out of {total} papers"
                }
                button {
                    class: "btn btn--ghost btn--xs",
                    onclick: move |_| oa_state.set(None),
                    "Dismiss"
                }
            }
        }
    }
}

#[component]
fn ExportBibtexButton() -> Element {
    let lib_state = use_context::<Signal<LibraryState>>();
    let mut status = use_signal(|| None::<String>);

    rsx! {
        button {
            class: "btn btn--ghost btn--sm",
            onclick: move |_| {
                let papers = lib_state.read().papers.clone();
                if papers.is_empty() {
                    status.set(Some("No papers to export".to_string()));
                    return;
                }

                let file = super::save_file(&["bib"], "Export BibTeX", "rotero-export.bib");

                if let Some(path) = file {
                    let bibtex = rotero_bib::export_bibtex(&papers);
                    match std::fs::write(&path, bibtex) {
                        Ok(()) => status.set(Some(format!("Exported {} papers", papers.len()))),
                        Err(e) => status.set(Some(format!("Write error: {e}"))),
                    }
                }
            },
            i { class: "bi bi-upload" }
            " Export"
        }
        if let Some(msg) = status.read().as_ref() {
            span { class: "import-status", "{msg}" }
        }
    }
}
