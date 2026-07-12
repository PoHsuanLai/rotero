use rotero_models::{Paper, PaperId};
use rotero_search::merge::merge_into;

/// Tries CrossRef first (most complete), then fills gaps from OpenAlex and Semantic Scholar.
pub async fn enrich_paper(paper: &Paper) -> Option<Paper> {
    if let Some(id) = paper.paper_id() {
        match &id {
            PaperId::ArXiv(arxiv) => fetch_from_sources_arxiv(arxiv).await,
            PaperId::Doi(doi) => fetch_from_sources_doi(doi).await,
            PaperId::Pmid(_) | PaperId::Isbn(_) => None,
        }
    } else if let Some(arxiv) = extract_arxiv_from_url(paper) {
        fetch_from_sources_arxiv(&arxiv).await
    } else if !paper.title.is_empty() && paper.title != "Untitled" {
        match super::openalex::search_by_title(&paper.title).await {
            Ok(paper) => Some(paper),
            Err(e) => {
                tracing::debug!("OpenAlex title search failed: {e}");
                None
            }
        }
    } else {
        None
    }
}

/// CrossRef -> OpenAlex -> Semantic Scholar. Merges abstract from secondary sources if missing.
async fn fetch_from_sources_doi(doi: &str) -> Option<Paper> {
    let mut primary = match super::crossref::fetch_by_doi(doi).await {
        Ok(paper) => Some(paper),
        Err(e) => {
            tracing::debug!("CrossRef failed for {doi}: {e}");
            None
        }
    };

    if primary
        .as_ref()
        .is_none_or(|p| p.abstract_text.is_none() || p.citation.citation_count.is_none())
    {
        match super::semantic_scholar::fetch_by_doi(doi).await {
            Ok(s2_paper) => match primary {
                Some(ref mut p) => merge_into(p, s2_paper),
                None => primary = Some(s2_paper),
            },
            Err(e) => tracing::debug!("Semantic Scholar failed for {doi}: {e}"),
        }
    }

    if primary.is_none() {
        match super::openalex::fetch_by_doi(doi).await {
            Ok(paper) => primary = Some(paper),
            Err(e) => tracing::debug!("OpenAlex failed for {doi}: {e}"),
        }
    }

    primary
}

/// arXiv API -> Semantic Scholar.
async fn fetch_from_sources_arxiv(arxiv_id: &str) -> Option<Paper> {
    let mut primary = match super::arxiv::fetch_by_arxiv_id(arxiv_id).await {
        Ok(paper) => Some(paper),
        Err(e) => {
            tracing::debug!("arXiv API failed for {arxiv_id}: {e}");
            None
        }
    };

    match super::semantic_scholar::fetch_by_arxiv_id(arxiv_id).await {
        Ok(s2_paper) => {
            if let Some(ref mut p) = primary {
                merge_into(p, s2_paper);
            } else {
                primary = Some(s2_paper);
            }
        }
        Err(e) => tracing::debug!("Semantic Scholar failed for arXiv:{arxiv_id}: {e}"),
    }

    primary
}

fn extract_arxiv_from_url(paper: &Paper) -> Option<String> {
    let url = paper.links.url.as_deref()?;
    let pos = url
        .find("arxiv.org/abs/")
        .or_else(|| url.find("arxiv.org/pdf/"))?;
    let after = &url[pos + 14..];
    let id: String = after
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    if id.contains('.') && !id.is_empty() {
        Some(id)
    } else {
        None
    }
}
