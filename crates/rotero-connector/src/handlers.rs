use axum::{Json, extract::Query, extract::State};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use super::ConnectorState;
use rotero_bib::citation::AVAILABLE_STYLES;

/// Incoming JSON body for `POST /api/save`.
#[derive(Debug, Deserialize)]
pub struct SavePaperRequest {
    pub url: Option<String>,
    pub doi: Option<String>,
    pub title: Option<String>,
    pub authors: Option<Vec<String>>,
    pub pdf_url: Option<String>,
    pub journal: Option<String>,
    pub year: Option<i32>,
    pub volume: Option<String>,
    pub issue: Option<String>,
    pub pages: Option<String>,
    pub publisher: Option<String>,
    pub abstract_text: Option<String>,
    pub collection_id: Option<String>,
    pub tag_ids: Option<Vec<String>>,
}

/// JSON response returned by `POST /api/save`.
#[derive(Debug, Serialize)]
pub struct SavePaperResponse {
    pub success: bool,
    pub message: String,
    pub paper_id: Option<String>,
}

/// JSON response for `GET /api/status` health-check.
#[derive(Debug, Serialize)]
pub struct StatusResponse {
    pub status: &'static str,
    pub version: &'static str,
    pub name: &'static str,
}

/// Lightweight collection descriptor sent to the browser extension.
#[derive(Debug, Serialize)]
pub struct CollectionInfo {
    pub id: String,
    pub name: String,
}

/// JSON response for `GET /api/collections`.
#[derive(Debug, Serialize)]
pub struct CollectionsResponse {
    pub collections: Vec<CollectionInfo>,
}

/// Lightweight tag descriptor sent to the browser extension.
#[derive(Debug, Serialize)]
pub struct TagInfo {
    pub id: String,
    pub name: String,
    pub color: Option<String>,
}

/// JSON response for `GET /api/tags`.
#[derive(Debug, Serialize)]
pub struct TagsResponse {
    pub tags: Vec<TagInfo>,
}

/// Handler for `GET /api/status`. Returns app name and version.
pub async fn status() -> Json<StatusResponse> {
    Json(StatusResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
        name: "Rotero",
    })
}

/// Handler for `GET /api/collections`. Returns the user's collection list.
pub async fn collections(State(state): State<Arc<ConnectorState>>) -> Json<CollectionsResponse> {
    let collections = if let Some(ref callback) = state.on_get_collections {
        callback()
    } else {
        Vec::new()
    };
    Json(CollectionsResponse { collections })
}

/// Handler for `GET /api/tags`. Returns the user's tag list.
pub async fn tags(State(state): State<Arc<ConnectorState>>) -> Json<TagsResponse> {
    let tags = if let Some(ref callback) = state.on_get_tags {
        callback()
    } else {
        Vec::new()
    };
    Json(TagsResponse { tags })
}

/// Incoming JSON body for `POST /api/scrape`.
#[derive(Debug, Deserialize)]
pub struct ScrapeRequest {
    pub url: String,
}

/// JSON response for `POST /api/scrape`.
#[derive(Debug, Serialize)]
pub struct ScrapeResponse {
    pub success: bool,
    pub metadata: Option<ScrapeResult>,
    pub error: Option<String>,
}

/// Scraped paper metadata returned inside [`ScrapeResponse`].
#[derive(Debug, Serialize)]
pub struct ScrapeResult {
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub doi: Option<String>,
    pub url: Option<String>,
    pub pdf_url: Option<String>,
    pub journal: Option<String>,
    pub year: Option<i32>,
    pub volume: Option<String>,
    pub issue: Option<String>,
    pub pages: Option<String>,
    pub publisher: Option<String>,
    pub abstract_text: Option<String>,
}

/// Handler for `POST /api/scrape`. Tries the translation server first,
/// then falls back to HTML meta-tag scraping.
pub async fn scrape(
    State(state): State<Arc<ConnectorState>>,
    Json(req): Json<ScrapeRequest>,
) -> Json<ScrapeResponse> {
    // Try Zotero translation server first (much better coverage)
    {
        let ts_guard = state.translation_server.read().await;
        if let Some(ref ts) = *ts_guard {
            match ts.translate_web(&req.url).await {
                Ok(items) => {
                    if let Some(item) = items.iter().find(|i| {
                        i.item_type != "note" && i.item_type != "attachment" && !i.title.is_empty()
                    }) {
                        let pdf_url = item.pdf_url();
                        if let Some(p) = item.clone().into_paper() {
                            return Json(ScrapeResponse {
                                success: true,
                                metadata: Some(ScrapeResult {
                                    title: Some(p.title.clone()),
                                    authors: p.authors.clone(),
                                    doi: p.doi.clone(),
                                    url: p.links.url.clone(),
                                    pdf_url,
                                    journal: p.publication.journal.clone(),
                                    year: p.year,
                                    volume: p.publication.volume.clone(),
                                    issue: p.publication.issue.clone(),
                                    pages: p.publication.pages.clone(),
                                    publisher: p.publication.publisher.clone(),
                                    abstract_text: p.abstract_text.clone(),
                                }),
                                error: None,
                            });
                        }
                    }
                    tracing::debug!(
                        "Translation server returned no usable results for {}, falling back",
                        req.url
                    );
                }
                Err(e) => {
                    tracing::debug!(
                        "Translation server error for {}: {e}, falling back",
                        req.url
                    );
                }
            }
        }
    }

    // Fallback: meta-tag scraper
    match super::scrape::scrape_url(&req.url).await {
        Ok(p) => Json(ScrapeResponse {
            success: true,
            metadata: Some(ScrapeResult {
                title: Some(p.title.clone()),
                authors: p.authors.clone(),
                doi: p.doi.clone(),
                url: p.links.url.clone(),
                pdf_url: p.links.pdf_url.clone(),
                journal: p.publication.journal.clone(),
                year: p.year,
                volume: p.publication.volume.clone(),
                issue: p.publication.issue.clone(),
                pages: p.publication.pages.clone(),
                publisher: p.publication.publisher.clone(),
                abstract_text: p.abstract_text.clone(),
            }),
            error: None,
        }),
        Err(e) => Json(ScrapeResponse {
            success: false,
            metadata: None,
            error: Some(e),
        }),
    }
}

// ---------------------------------------------------------------------------
// Citation API types
// ---------------------------------------------------------------------------

/// A style entry returned by `GET /api/cite/styles`.
#[derive(Debug, Serialize)]
pub struct StyleEntry {
    pub id: String,
    pub name: String,
}

/// JSON response for `GET /api/cite/styles`.
#[derive(Debug, Serialize)]
pub struct StylesResponse {
    pub styles: Vec<StyleEntry>,
}

/// Query parameter for `GET /api/cite/search`.
#[derive(Debug, Deserialize)]
pub struct CiteSearchQuery {
    pub q: String,
}

/// Lightweight paper info returned by search.
#[derive(Debug, Serialize)]
pub struct CiteSearchPaper {
    pub id: String,
    pub title: String,
    pub authors: Vec<String>,
    pub year: Option<i32>,
    pub doi: Option<String>,
    pub journal: Option<String>,
}

/// JSON response for `GET /api/cite/search`.
#[derive(Debug, Serialize)]
pub struct CiteSearchResponse {
    pub papers: Vec<CiteSearchPaper>,
}

/// JSON request body for `POST /api/cite/format`.
#[derive(Debug, Deserialize)]
pub struct CiteFormatRequest {
    pub paper_ids: Vec<String>,
    pub style: String,
}

/// Per-paper citation text.
#[derive(Debug, Serialize)]
pub struct CitationEntry {
    pub paper_id: String,
    pub text: String,
}

/// JSON response for `POST /api/cite/format`.
#[derive(Debug, Serialize)]
pub struct CiteFormatResponse {
    pub success: bool,
    pub citations: Vec<CitationEntry>,
    pub combined: String,
    pub error: Option<String>,
}

/// JSON request body for `POST /api/cite/bibliography`.
#[derive(Debug, Deserialize)]
pub struct CiteBibliographyRequest {
    pub paper_ids: Vec<String>,
    pub style: String,
}

/// JSON response for `POST /api/cite/bibliography`.
#[derive(Debug, Serialize)]
pub struct CiteBibliographyResponse {
    pub success: bool,
    pub entries: Vec<CitationEntry>,
    pub error: Option<String>,
}

/// Convert a display name to a URL-safe slug (e.g. "APA 7th" → "apa-7th").
fn style_slug(name: &str) -> String {
    name.to_lowercase().replace(' ', "-")
}

/// Resolve a style slug back to an `ArchivedStyle`.
fn resolve_style(slug: &str) -> Option<rotero_bib::ArchivedStyle> {
    AVAILABLE_STYLES
        .iter()
        .find(|(name, _)| style_slug(name) == slug)
        .map(|(_, style)| *style)
}

/// Handler for `GET /api/cite/styles`. Returns all available CSL styles.
pub async fn cite_styles() -> Json<StylesResponse> {
    let styles = AVAILABLE_STYLES
        .iter()
        .map(|(name, _)| StyleEntry {
            id: style_slug(name),
            name: name.to_string(),
        })
        .collect();
    Json(StylesResponse { styles })
}

/// Handler for `GET /api/cite/search?q=...`. Searches papers in the library.
pub async fn cite_search(
    State(state): State<Arc<ConnectorState>>,
    Query(query): Query<CiteSearchQuery>,
) -> Json<CiteSearchResponse> {
    let papers = if let Some(ref callback) = state.on_search_papers {
        callback(&query.q)
    } else {
        Vec::new()
    };
    let results = papers
        .into_iter()
        .filter_map(|p| {
            Some(CiteSearchPaper {
                id: p.id?,
                title: p.title,
                authors: p.authors,
                year: p.year,
                doi: p.doi,
                journal: p.publication.journal,
            })
        })
        .collect();
    Json(CiteSearchResponse { papers: results })
}

/// Handler for `POST /api/cite/format`. Returns inline citation text.
pub async fn cite_format(
    State(state): State<Arc<ConnectorState>>,
    Json(req): Json<CiteFormatRequest>,
) -> Json<CiteFormatResponse> {
    let style = match resolve_style(&req.style) {
        Some(s) => s,
        None => {
            return Json(CiteFormatResponse {
                success: false,
                citations: Vec::new(),
                combined: String::new(),
                error: Some(format!("Unknown style: {}", req.style)),
            });
        }
    };

    let papers = if let Some(ref callback) = state.on_get_papers_by_ids {
        callback(&req.paper_ids)
    } else {
        Vec::new()
    };

    match rotero_bib::format_inline_citations(&papers, style) {
        Ok((individual, combined)) => {
            let citations = papers
                .iter()
                .zip(individual)
                .filter_map(|(p, text)| {
                    Some(CitationEntry {
                        paper_id: p.id.clone()?,
                        text,
                    })
                })
                .collect();
            Json(CiteFormatResponse {
                success: true,
                citations,
                combined,
                error: None,
            })
        }
        Err(e) => Json(CiteFormatResponse {
            success: false,
            citations: Vec::new(),
            combined: String::new(),
            error: Some(e),
        }),
    }
}

/// Handler for `POST /api/cite/bibliography`. Returns formatted bibliography entries.
pub async fn cite_bibliography(
    State(state): State<Arc<ConnectorState>>,
    Json(req): Json<CiteBibliographyRequest>,
) -> Json<CiteBibliographyResponse> {
    let style = match resolve_style(&req.style) {
        Some(s) => s,
        None => {
            return Json(CiteBibliographyResponse {
                success: false,
                entries: Vec::new(),
                error: Some(format!("Unknown style: {}", req.style)),
            });
        }
    };

    let papers = if let Some(ref callback) = state.on_get_papers_by_ids {
        callback(&req.paper_ids)
    } else {
        Vec::new()
    };

    match rotero_bib::format_bibliography_entries(&papers, style) {
        Ok(texts) => {
            let entries = papers
                .iter()
                .zip(texts)
                .filter_map(|(p, text)| {
                    Some(CitationEntry {
                        paper_id: p.id.clone()?,
                        text,
                    })
                })
                .collect();
            Json(CiteBibliographyResponse {
                success: true,
                entries,
                error: None,
            })
        }
        Err(e) => Json(CiteBibliographyResponse {
            success: false,
            entries: Vec::new(),
            error: Some(e),
        }),
    }
}

/// Handler for `POST /api/save`. Creates a [`Paper`](rotero_models::Paper) and
/// invokes the `on_paper_saved` callback to persist it.
pub async fn save_paper(
    State(state): State<Arc<ConnectorState>>,
    Json(req): Json<SavePaperRequest>,
) -> Json<SavePaperResponse> {
    let paper = rotero_models::Paper {
        title: req.title.unwrap_or_else(|| "Untitled".to_string()),
        authors: req.authors.unwrap_or_default(),
        year: req.year,
        doi: req.doi,
        abstract_text: req.abstract_text,
        publication: rotero_models::Publication {
            journal: req.journal,
            volume: req.volume,
            issue: req.issue,
            pages: req.pages,
            publisher: req.publisher,
        },
        links: rotero_models::PaperLinks {
            url: req.url,
            ..Default::default()
        },
        ..Default::default()
    };

    if let Some(ref callback) = state.on_paper_saved {
        callback(
            paper.clone(),
            req.collection_id,
            req.tag_ids.unwrap_or_default(),
            req.pdf_url,
        );
    }

    Json(SavePaperResponse {
        success: true,
        message: "Paper added to library".to_string(),
        paper_id: paper.id,
    })
}
