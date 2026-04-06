use std::sync::mpsc;

use dioxus::prelude::*;

use super::{recv_reply, RenderRequest};
use crate::state::app_state::LibraryState;

pub async fn extract_and_fetch_metadata(
    render_tx: &mpsc::Sender<RenderRequest>,
    conn: &rotero_db::turso::Connection,
    paper_id: &str,
    pdf_path: &str,
    auto_fetch: bool,
    lib_state: &mut Signal<LibraryState>,
) {
    tracing::info!(%paper_id, pdf_path, auto_fetch, "extract_and_fetch_metadata: start");
    let (reply_tx, reply_rx) = mpsc::channel();
    if render_tx
        .send(RenderRequest::ExtractMetadataText {
            pdf_path: pdf_path.to_string(),
            page_count: 2,
            reply: reply_tx,
        })
        .is_err()
    {
        tracing::warn!("extract_and_fetch_metadata: failed to send render request");
        return;
    }
    let Ok((raw_pages, doc_meta)) = recv_reply(reply_rx).await else {
        tracing::warn!("extract_and_fetch_metadata: render thread reply failed");
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
            Ok(meta) => {
                tracing::info!(title = %meta.title, authors = ?meta.authors, "extract_and_fetch_metadata: CrossRef success");
                let fetched = crate::metadata::parser::metadata_to_paper(meta);
                if apply_fetched_metadata(conn, paper_id, &fetched, lib_state).await {
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
            Ok(meta) => {
                tracing::info!(title = %meta.title, authors = ?meta.authors, "extract_and_fetch_metadata: arXiv success");
                let fetched = crate::metadata::parser::metadata_to_paper(meta);
                if apply_fetched_metadata(conn, paper_id, &fetched, lib_state).await {
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
                p.authors = author
                    .split(';')
                    .flat_map(|s| s.split(','))
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
            if let Some(ref doi_str) = doi {
                p.doi = Some(doi_str.clone());
            } else if let Some(ref arxiv) = arxiv_id {
                p.doi = Some(format!("arXiv:{arxiv}"));
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
        let _ = rotero_db::papers::update_paper_metadata(conn, paper_id, &paper).await;
    }
}

async fn apply_fetched_metadata(
    conn: &rotero_db::turso::Connection,
    paper_id: &str,
    fetched: &rotero_models::Paper,
    lib_state: &mut Signal<LibraryState>,
) -> bool {
    if rotero_db::papers::update_paper_metadata(conn, paper_id, fetched)
        .await
        .is_err()
    {
        return false;
    }
    lib_state.with_mut(|s| {
        if let Some(p) = s
            .papers
            .iter_mut()
            .find(|p| p.id.as_ref().map(|id| id.to_string()) == Some(paper_id.to_string()))
        {
            p.title = fetched.title.clone();
            p.authors = fetched.authors.clone();
            p.year = fetched.year;
            p.doi = fetched.doi.clone();
            p.abstract_text = fetched.abstract_text.clone();
            p.journal = fetched.journal.clone();
            p.volume = fetched.volume.clone();
            p.issue = fetched.issue.clone();
            p.pages = fetched.pages.clone();
            p.publisher = fetched.publisher.clone();
            p.url = fetched.url.clone();
            if fetched.citation_count.is_some() {
                p.citation_count = fetched.citation_count;
            }
        }
    });
    if let Some(count) = fetched.citation_count {
        let _ = rotero_db::papers::update_citation_count(conn, paper_id, count).await;
    }
    true
}
