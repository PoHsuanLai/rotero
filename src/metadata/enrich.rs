use rotero_models::Paper;

/// Tries CrossRef first (most complete), then fills gaps from OpenAlex and Semantic Scholar.
pub async fn enrich_paper(paper: &Paper) -> Option<Paper> {
    let doi = paper.doi.as_deref();
    let arxiv_id = extract_arxiv_id(paper);

    if let Some(doi) = doi {
        if doi.starts_with("arXiv:") {
            let id = doi.strip_prefix("arXiv:").unwrap_or(doi);
            fetch_from_sources_arxiv(id).await
        } else {
            fetch_from_sources_doi(doi).await
        }
    } else if let Some(arxiv) = arxiv_id {
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
                Some(ref mut p) => {
                    if p.abstract_text.is_none() {
                        p.abstract_text = s2_paper.abstract_text;
                    }
                    if p.citation.citation_count.is_none() {
                        p.citation.citation_count = s2_paper.citation.citation_count;
                    }
                }
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
                if p.abstract_text.is_none() {
                    p.abstract_text = s2_paper.abstract_text;
                }
                if p.year.is_none() {
                    p.year = s2_paper.year;
                }
                if p.citation.citation_count.is_none() {
                    p.citation.citation_count = s2_paper.citation.citation_count;
                }
            } else {
                primary = Some(s2_paper);
            }
        }
        Err(e) => tracing::debug!("Semantic Scholar failed for arXiv:{arxiv_id}: {e}"),
    }

    primary
}

fn extract_arxiv_id(paper: &Paper) -> Option<String> {
    if let Some(ref url) = paper.links.url
        && let Some(pos) = url
            .find("arxiv.org/abs/")
            .or_else(|| url.find("arxiv.org/pdf/"))
    {
        let after = &url[pos + 14..];
        let id: String = after
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect();
        if id.contains('.') && !id.is_empty() {
            return Some(id);
        }
    }
    if let Some(ref doi) = paper.doi
        && let Some(id) = doi.strip_prefix("arXiv:")
    {
        return Some(id.to_string());
    }
    None
}
