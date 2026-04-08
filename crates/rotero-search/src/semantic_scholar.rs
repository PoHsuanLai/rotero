use rotero_models::{CitationInfo, Paper, Publication};
use serde::Deserialize;

const S2_API: &str = "https://api.semanticscholar.org/graph/v1/paper";
const S2_FIELDS: &str =
    "title,authors,year,abstract,venue,externalIds,publicationVenue,citationCount";

#[derive(Debug, Deserialize)]
struct S2Paper {
    title: Option<String>,
    authors: Option<Vec<S2Author>>,
    year: Option<i32>,
    #[serde(rename = "abstract")]
    abstract_text: Option<String>,
    venue: Option<String>,
    #[serde(rename = "externalIds")]
    external_ids: Option<S2ExternalIds>,
    #[serde(rename = "publicationVenue")]
    publication_venue: Option<S2Venue>,
    #[serde(rename = "citationCount")]
    citation_count: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct S2Author {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct S2ExternalIds {
    #[serde(rename = "DOI")]
    doi: Option<String>,
    #[allow(dead_code)]
    #[serde(rename = "ArXiv")]
    arxiv: Option<String>,
}

#[derive(Debug, Deserialize)]
struct S2Venue {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct S2SearchResponse {
    data: Option<Vec<S2Paper>>,
}

/// Searches Semantic Scholar for papers matching the given query.
pub async fn search_papers(query: &str, limit: usize) -> Result<Vec<Paper>, String> {
    let url = format!(
        "https://api.semanticscholar.org/graph/v1/paper/search?query={}&limit={limit}&fields={S2_FIELDS}",
        urlencoding::encode(query)
    );

    let client = crate::shared_client();
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Semantic Scholar request failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!(
            "Semantic Scholar API returned status {}",
            resp.status()
        ));
    }

    let data: S2SearchResponse = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse Semantic Scholar response: {e}"))?;

    let papers = data.data.unwrap_or_default();
    let mut results = Vec::new();
    for paper in papers {
        let doi = paper
            .external_ids
            .as_ref()
            .and_then(|e| e.doi.clone())
            .unwrap_or_default();
        if let Ok(p) = s2_to_paper(paper, &doi) {
            results.push(p);
        }
    }
    Ok(results)
}

/// Fetches paper metadata from Semantic Scholar by DOI.
pub async fn fetch_by_doi(doi: &str) -> Result<Paper, String> {
    let url = format!("{S2_API}/DOI:{doi}?fields={S2_FIELDS}");
    let paper = fetch_paper(&url).await?;
    s2_to_paper(paper, doi)
}

/// Fetches paper metadata from Semantic Scholar by arXiv ID.
pub async fn fetch_by_arxiv_id(arxiv_id: &str) -> Result<Paper, String> {
    let url = format!("{S2_API}/ARXIV:{arxiv_id}?fields={S2_FIELDS}");
    let paper = fetch_paper(&url).await?;
    let doi = paper
        .external_ids
        .as_ref()
        .and_then(|e| e.doi.clone())
        .unwrap_or_default();
    s2_to_paper(paper, &doi)
}

async fn fetch_paper(url: &str) -> Result<S2Paper, String> {
    let resp = crate::shared_client()
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Semantic Scholar request failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!(
            "Semantic Scholar API returned status {}",
            resp.status()
        ));
    }

    resp.json()
        .await
        .map_err(|e| format!("Failed to parse Semantic Scholar response: {e}"))
}

fn s2_to_paper(paper: S2Paper, doi: &str) -> Result<Paper, String> {
    let title = paper.title.unwrap_or_default();
    if title.is_empty() {
        return Err("Empty title from Semantic Scholar".to_string());
    }

    let authors: Vec<String> = paper
        .authors
        .unwrap_or_default()
        .into_iter()
        .filter_map(|a| a.name)
        .collect();

    let journal = paper
        .publication_venue
        .and_then(|v| v.name)
        .or(paper.venue.filter(|v| !v.is_empty()));

    Ok(Paper {
        title,
        authors,
        year: paper.year,
        doi: if doi.is_empty() {
            None
        } else {
            Some(doi.to_string())
        },
        abstract_text: paper.abstract_text,
        publication: Publication {
            journal,
            ..Default::default()
        },
        citation: CitationInfo {
            citation_count: paper.citation_count,
            ..Default::default()
        },
        ..Default::default()
    })
}
