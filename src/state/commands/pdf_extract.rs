use std::collections::HashMap;

use dioxus::prelude::*;

use super::PdfDocs;
use crate::state::app_state::{LibraryState, PdfTabManager, TabId};

pub async fn extract_and_fetch_metadata(
    docs: &PdfDocs,
    db: &rotero_db::Database,
    paper_id: &str,
    pdf_path: &str,
    auto_fetch: bool,
    lib_state: &mut Signal<LibraryState>,
) {
    tracing::info!(%paper_id, pdf_path, auto_fetch, "extract_and_fetch_metadata: start");
    let Ok((raw_pages, doc_meta)) = docs.extract_metadata_text(pdf_path.to_string(), 2).await
    else {
        tracing::warn!("extract_and_fetch_metadata: PDF text extract failed");
        return;
    };
    tracing::info!(pages = raw_pages.len(), total_chars = raw_pages.iter().map(|(_, t)| t.len()).sum::<usize>(), doc_title = ?doc_meta.title, doc_author = ?doc_meta.author, "extract_and_fetch_metadata: text extracted");
    let combined_text: String = raw_pages
        .iter()
        .map(|(_, t)| t.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let doi = crate::metadata::doi_extract::extract_doi(&combined_text);
    let arxiv_id = crate::metadata::doi_extract::extract_arxiv_id(&combined_text);
    tracing::info!(?doi, ?arxiv_id, "extract_and_fetch_metadata: ID extraction");
    if let Some(ref doi_str) = doi
        && auto_fetch
    {
        match crate::metadata::crossref::fetch_by_doi(doi_str).await {
            Ok(fetched) => {
                tracing::info!(title = %fetched.title, authors = ?fetched.author_names(), "extract_and_fetch_metadata: CrossRef success");
                if apply_fetched_metadata(db, paper_id, &fetched, lib_state).await {
                    return;
                }
            }
            Err(e) => {
                tracing::warn!(%e, "extract_and_fetch_metadata: CrossRef lookup failed");
            }
        }
    }
    if let Some(ref arxiv) = arxiv_id
        && auto_fetch
    {
        match crate::metadata::arxiv::fetch_by_arxiv_id(arxiv).await {
            Ok(fetched) => {
                tracing::info!(title = %fetched.title, authors = ?fetched.author_names(), "extract_and_fetch_metadata: arXiv success");
                if apply_fetched_metadata(db, paper_id, &fetched, lib_state).await {
                    return;
                }
            }
            Err(e) => {
                tracing::warn!(%e, "extract_and_fetch_metadata: arXiv lookup failed");
            }
        }
    }
    let has_update = doc_meta.title.is_some()
        || doc_meta.author.is_some()
        || doi.is_some()
        || arxiv_id.is_some();
    if !has_update {
        tracing::info!("extract_and_fetch_metadata: no metadata found");
        return;
    }
    lib_state.with_mut(|s| {
        if let Some(p) = s
            .papers
            .iter_mut()
            .find(|p| p.id.as_ref().map(|id| id.to_string()) == Some(paper_id.to_string()))
        {
            if let Some(ref title) = doc_meta.title {
                p.title = title.clone();
            }
            if let Some(ref author) = doc_meta.author {
                p.creators = author
                    .split(';')
                    .flat_map(|s| s.split(','))
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .map(rotero_models::Creator::author_from_display)
                    .collect();
            }
            if let Some(ref doi_str) = doi {
                p.doi = Some(doi_str.clone());
            } else if let Some(ref arxiv) = arxiv_id {
                p.doi = Some(rotero_models::PaperId::ArXiv(arxiv.clone()).to_stored_string());
            }
        }
    });
    let paper_snapshot = lib_state
        .read()
        .papers
        .iter()
        .find(|p| p.id.as_ref().map(|id| id.to_string()) == Some(paper_id.to_string()))
        .cloned();
    if let Some(paper) = paper_snapshot {
        let _ = db.update_paper_metadata(paper_id, &paper).await;
    }
}

async fn apply_fetched_metadata(
    db: &rotero_db::Database,
    paper_id: &str,
    fetched: &rotero_models::Paper,
    lib_state: &mut Signal<LibraryState>,
) -> bool {
    if db.update_paper_metadata(paper_id, fetched).await.is_err() {
        return false;
    }
    lib_state.with_mut(|s| {
        if let Some(p) = s
            .papers
            .iter_mut()
            .find(|p| p.id.as_ref().map(|id| id.to_string()) == Some(paper_id.to_string()))
        {
            p.title = fetched.title.clone();
            p.creators = fetched.creators.clone();
            p.year = fetched.year;
            p.doi = fetched.doi.clone();
            p.abstract_text = fetched.abstract_text.clone();
            p.publication = fetched.publication.clone();
            p.links.url = fetched.links.url.clone();
            if fetched.citation.citation_count.is_some() {
                p.citation.citation_count = fetched.citation.citation_count;
            }
        }
    });
    if let Some(count) = fetched.citation.citation_count {
        let _ = db.update_citation_count(paper_id, count).await;
    }
    true
}

/// Import annotations embedded in the PDF that are not already in the library.
///
/// Deduplicates against existing DB rows on the same page, type, and a ~10px
/// position match (PDF extract is in points; stored geometry is in page pixels).
pub async fn import_embedded_annotations(
    docs: &PdfDocs,
    db: &rotero_db::Database,
    mut tabs: Signal<PdfTabManager>,
    tab_id: TabId,
    paper_id: &str,
) {
    let mut anns = db
        .list_annotations_for_paper(paper_id)
        .await
        .unwrap_or_default();
    let (pdf_path, page_dims) = {
        let mgr = tabs.read();
        let Some(tab) = mgr.get(tab_id) else {
            return;
        };
        let dims: HashMap<u32, (u32, u32)> = tab
            .render
            .rendered_pages
            .values()
            .map(|p| (p.page_index, (p.width, p.height)))
            .collect();
        (tab.pdf_path.clone(), dims)
    };

    let Ok(extracted) = docs.extract_annotations(pdf_path).await else {
        tabs.with_mut(|m| {
            if let Some(t) = m.get_mut(tab_id) {
                t.annotations = anns;
            }
        });
        return;
    };

    let now = chrono::Utc::now();
    for ext in extracted {
        if extracted_ann_duplicates(&anns, &ext, &page_dims) {
            continue;
        }
        let (rw, rh) = page_dims.get(&ext.page).copied().unwrap_or((1, 1));
        let sx = rw as f32 / ext.page_width_pts;
        let sy = rh as f32 / ext.page_height_pts;
        let x = ext.rect_pts[0] * sx;
        let y = (ext.page_height_pts - ext.rect_pts[3]) * sy;
        let w = (ext.rect_pts[2] - ext.rect_pts[0]) * sx;
        let h = (ext.rect_pts[3] - ext.rect_pts[1]) * sy;

        let mut geometry = serde_json::json!({
            "x": x, "y": y, "width": w, "height": h,
            "page_width": rw, "page_height": rh,
        });
        if !ext.rects_pts.is_empty() {
            let rects: Vec<serde_json::Value> = ext
                .rects_pts
                .iter()
                .map(|r| {
                    let rx = r[0] * sx;
                    let ry = (ext.page_height_pts - r[3]) * sy;
                    let rect_w = (r[2] - r[0]) * sx;
                    let rect_h = (r[3] - r[1]) * sy;
                    serde_json::json!({
                        "x": rx, "y": ry, "width": rect_w, "height": rect_h,
                    })
                })
                .collect();
            geometry["rects"] = serde_json::Value::Array(rects);
        }

        let ann = rotero_models::Annotation {
            id: None,
            paper_id: paper_id.to_string(),
            page: ext.page as i32,
            ann_type: ext.ann_type,
            color: ext.color,
            content: ext.content,
            geometry,
            created_at: now,
            modified_at: now,
        };
        if let Ok(id) = db.insert_annotation(&ann).await {
            let mut ann = ann;
            ann.id = Some(id);
            anns.push(ann);
        }
    }

    tabs.with_mut(|m| {
        if let Some(t) = m.get_mut(tab_id) {
            t.annotations = anns;
        }
    });
}

fn extracted_ann_duplicates(
    anns: &[rotero_models::Annotation],
    ext: &rotero_pdf::ExtractedAnnotation,
    page_dims: &HashMap<u32, (u32, u32)>,
) -> bool {
    anns.iter().any(|a| {
        a.page == ext.page as i32 && a.ann_type == ext.ann_type && {
            let ax = a.geometry.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let ay = a.geometry.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let (rw, rh) = page_dims.get(&ext.page).copied().unwrap_or((1, 1));
            let sx = rw as f64 / ext.page_width_pts as f64;
            let sy = rh as f64 / ext.page_height_pts as f64;
            let ex = ext.rect_pts[0] as f64 * sx;
            let ey = (ext.page_height_pts as f64 - ext.rect_pts[3] as f64) * sy;
            (ax - ex).abs() < 10.0 && (ay - ey).abs() < 10.0
        }
    })
}
