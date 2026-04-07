use dioxus::prelude::*;

use crate::state::app_state::LibraryState;
use rotero_db::Database;

#[component]
pub fn ImportExportButtons() -> Element {
    rsx! {
        div { class: "import-export-row",
            ImportButton {}
            ExportBibtexButton {}
        }
    }
}

#[component]
fn ImportButton() -> Element {
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let db = use_context::<Database>();
    let mut status = use_signal(|| None::<String>);

    rsx! {
        button {
            class: "btn btn--ghost btn--sm",
            onclick: move |_| {
                let db = db.clone();
                spawn(async move {
                    let file = super::pick_file_async(
                        &["bib", "bibtex", "ris", "json"],
                        "Import Library",
                    ).await;
                    if let Some(path) = file {
                        match std::fs::read_to_string(&path) {
                            Ok(content) => {
                                let ext = path.extension()
                                    .and_then(|e| e.to_str())
                                    .unwrap_or("")
                                    .to_lowercase();

                                // Directory containing the bib file (for resolving relative PDF paths)
                                let bib_dir = path.parent().map(|p| p.to_path_buf());

                                match ext.as_str() {
                                    "ris" | "json" => {
                                        let result = if ext == "ris" {
                                            rotero_bib::import_ris(&content)
                                        } else {
                                            rotero_bib::import_csl_json(&content)
                                        };
                                        match result {
                                            Ok(papers) => {
                                                let count = papers.len();
                                                let mut imported = 0;
                                                for paper in papers {
                                                    if let Ok(id) = rotero_db::papers::insert_paper(db.conn(), &paper).await {
                                                        let mut paper = paper;
                                                        paper.id = Some(id);
                                                        lib_state.with_mut(|s| s.papers.insert(0, paper));
                                                        imported += 1;
                                                    }
                                                }
                                                status.set(Some(format!("Imported {imported}/{count} papers")));
                                            }
                                            Err(e) => status.set(Some(format!("Parse error: {e}"))),
                                        }
                                    }
                                    _ => {
                                        // BibTeX import with PDF support
                                        match rotero_bib::import_bibtex(&content) {
                                            Ok(entries) => {
                                                let count = entries.len();
                                                let mut imported = 0;
                                                let mut pdfs_imported = 0;
                                                for entry in entries {
                                                    let rotero_bib::ImportedPaper { paper, source_pdf } = entry;
                                                    if let Ok(id) = rotero_db::papers::insert_paper(db.conn(), &paper).await {
                                                        let mut paper = paper;
                                                        paper.id = Some(id.clone());

                                                        // Try to import the associated PDF
                                                        if let (Some(bib_dir), Some(rel_pdf)) = (&bib_dir, &source_pdf) {
                                                            let pdf_abs = bib_dir.join(rel_pdf);
                                                            if pdf_abs.exists() {
                                                                if let Ok(rel_path) = db.import_pdf(
                                                                    pdf_abs.to_str().unwrap_or_default(),
                                                                    Some(paper.title.as_str()),
                                                                    paper.authors.first().map(|a| a.as_str()),
                                                                    paper.year,
                                                                ) {
                                                                    let _ = rotero_db::papers::update_pdf_path(db.conn(), &id, &rel_path).await;
                                                                    paper.links.pdf_path = Some(rel_path);
                                                                    pdfs_imported += 1;
                                                                }
                                                            }
                                                        }

                                                        lib_state.with_mut(|s| s.papers.insert(0, paper));
                                                        imported += 1;
                                                    }
                                                }
                                                let pdf_msg = if pdfs_imported > 0 {
                                                    format!(" ({pdfs_imported} PDFs)")
                                                } else {
                                                    String::new()
                                                };
                                                status.set(Some(format!("Imported {imported}/{count} papers{pdf_msg}")));
                                            }
                                            Err(e) => status.set(Some(format!("Parse error: {e}"))),
                                        }
                                    }
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
