use rotero_models::Paper;

use rotero_search::FetchedMetadata;

/// Enrich a paper with metadata from multiple sources.
/// Tries CrossRef first (most complete), then fills gaps from OpenAlex and Semantic Scholar.
pub async fn enrich_paper(paper: &Paper) -> Option<Paper> {
    let doi = paper.doi.as_deref();
    let arxiv_id = extract_arxiv_id(paper);

    // Try to get metadata from primary sources
    let meta = if let Some(doi) = doi {
        if doi.starts_with("arXiv:") {
            // arXiv pseudo-DOI — use arXiv API
            let id = doi.strip_prefix("arXiv:").unwrap_or(doi);
            fetch_from_sources_arxiv(id).await
        } else {
            fetch_from_sources_doi(doi).await
        }
    } else if let Some(arxiv) = arxiv_id {
        fetch_from_sources_arxiv(&arxiv).await
    } else if !paper.title.is_empty() && paper.title != "Untitled" {
        // Title-only: search OpenAlex
        match super::openalex::search_by_title(&paper.title).await {
            Ok(meta) => Some(meta),
            Err(e) => {
                tracing::debug!("OpenAlex title search failed: {e}");
                None
            }
        }
    } else {
        None
    };

    meta.map(super::parser::metadata_to_paper)
}

/// Try DOI-based sources: CrossRef → OpenAlex → Semantic Scholar.
/// Returns the first successful result, but merges abstract from secondary sources if missing.
async fn fetch_from_sources_doi(doi: &str) -> Option<FetchedMetadata> {
    // CrossRef is the primary source (has the most structured fields)
    let mut primary = match super::crossref::fetch_by_doi(doi).await {
        Ok(meta) => Some(meta),
        Err(e) => {
            tracing::debug!("CrossRef failed for {doi}: {e}");
            None
        }
    };

    // Try Semantic Scholar for abstract and citation count
    if primary
        .as_ref()
        .is_none_or(|m| m.abstract_text.is_none() || m.citation_count.is_none())
    {
        match super::semantic_scholar::fetch_by_doi(doi).await {
            Ok(s2_meta) => match primary {
                Some(ref mut p) => {
                    if p.abstract_text.is_none() {
                        p.abstract_text = s2_meta.abstract_text;
                    }
                    if p.citation_count.is_none() {
                        p.citation_count = s2_meta.citation_count;
                    }
                }
                None => primary = Some(s2_meta),
            },
            Err(e) => tracing::debug!("Semantic Scholar failed for {doi}: {e}"),
        }
    }

    // If still no result, try OpenAlex
    if primary.is_none() {
        match super::openalex::fetch_by_doi(doi).await {
            Ok(meta) => primary = Some(meta),
            Err(e) => tracing::debug!("OpenAlex failed for {doi}: {e}"),
        }
    }

    primary
}

/// Try arXiv-based sources: arXiv API → Semantic Scholar.
async fn fetch_from_sources_arxiv(arxiv_id: &str) -> Option<FetchedMetadata> {
    let mut primary = match super::arxiv::fetch_by_arxiv_id(arxiv_id).await {
        Ok(meta) => Some(meta),
        Err(e) => {
            tracing::debug!("arXiv API failed for {arxiv_id}: {e}");
            None
        }
    };

    // Semantic Scholar often has richer data for arXiv papers (abstract, year, citation count)
    match super::semantic_scholar::fetch_by_arxiv_id(arxiv_id).await {
        Ok(s2_meta) => {
            if let Some(ref mut p) = primary {
                if p.abstract_text.is_none() {
                    p.abstract_text = s2_meta.abstract_text;
                }
                if p.year.is_none() {
                    p.year = s2_meta.year;
                }
                if p.citation_count.is_none() {
                    p.citation_count = s2_meta.citation_count;
                }
            } else {
                primary = Some(s2_meta);
            }
        }
        Err(e) => tracing::debug!("Semantic Scholar failed for arXiv:{arxiv_id}: {e}"),
    }

    primary
}

fn extract_arxiv_id(paper: &Paper) -> Option<String> {
    // Check URL for arXiv ID
    if let Some(ref url) = paper.url {
        // Match patterns like arxiv.org/abs/1234.5678 or arxiv.org/pdf/1234.5678
        if let Some(pos) = url
            .find("arxiv.org/abs/")
            .or_else(|| url.find("arxiv.org/pdf/"))
        {
            let after = &url[pos + 14..]; // len of "arxiv.org/abs/" or "arxiv.org/pdf/"
            let id: String = after
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .collect();
            if id.contains('.') && !id.is_empty() {
                return Some(id);
            }
        }
    }
    // Check DOI for arXiv pseudo-DOI
    if let Some(ref doi) = paper.doi
        && let Some(id) = doi.strip_prefix("arXiv:")
    {
        return Some(id.to_string());
    }
    None
}
