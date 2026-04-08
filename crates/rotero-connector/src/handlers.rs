use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use super::ConnectorState;

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
