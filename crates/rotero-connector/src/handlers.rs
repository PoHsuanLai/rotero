use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use super::ConnectorState;

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
    pub collection_id: Option<i64>,
    pub tag_ids: Option<Vec<i64>>,
}

#[derive(Debug, Serialize)]
pub struct SavePaperResponse {
    pub success: bool,
    pub message: String,
    pub paper_id: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct StatusResponse {
    pub status: &'static str,
    pub version: &'static str,
    pub name: &'static str,
}

#[derive(Debug, Serialize)]
pub struct CollectionInfo {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct CollectionsResponse {
    pub collections: Vec<CollectionInfo>,
}

#[derive(Debug, Serialize)]
pub struct TagInfo {
    pub id: i64,
    pub name: String,
    pub color: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TagsResponse {
    pub tags: Vec<TagInfo>,
}

pub async fn status() -> Json<StatusResponse> {
    Json(StatusResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
        name: "Rotero",
    })
}

pub async fn collections(
    State(state): State<Arc<ConnectorState>>,
) -> Json<CollectionsResponse> {
    let collections = if let Some(ref callback) = state.on_get_collections {
        callback()
    } else {
        Vec::new()
    };
    Json(CollectionsResponse { collections })
}

pub async fn tags(
    State(state): State<Arc<ConnectorState>>,
) -> Json<TagsResponse> {
    let tags = if let Some(ref callback) = state.on_get_tags {
        callback()
    } else {
        Vec::new()
    };
    Json(TagsResponse { tags })
}

#[derive(Debug, Deserialize)]
pub struct ScrapeRequest {
    pub url: String,
}

#[derive(Debug, Serialize)]
pub struct ScrapeResponse {
    pub success: bool,
    pub metadata: Option<ScrapeResult>,
    pub error: Option<String>,
}

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

pub async fn scrape(Json(req): Json<ScrapeRequest>) -> Json<ScrapeResponse> {
    match super::scrape::scrape_url(&req.url).await {
        Ok(meta) => Json(ScrapeResponse {
            success: true,
            metadata: Some(ScrapeResult {
                title: meta.title,
                authors: meta.authors,
                doi: meta.doi,
                url: meta.url,
                pdf_url: meta.pdf_url,
                journal: meta.journal,
                year: meta.year,
                volume: meta.volume,
                issue: meta.issue,
                pages: meta.pages,
                publisher: meta.publisher,
                abstract_text: meta.abstract_text,
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

pub async fn save_paper(
    State(state): State<Arc<ConnectorState>>,
    Json(req): Json<SavePaperRequest>,
) -> Json<SavePaperResponse> {
    let title = req.title.unwrap_or_else(|| "Untitled".to_string());
    let mut paper = rotero_models::Paper::new(title);
    paper.doi = req.doi;
    paper.url = req.url;
    paper.journal = req.journal;
    paper.year = req.year;
    paper.volume = req.volume;
    paper.issue = req.issue;
    paper.pages = req.pages;
    paper.publisher = req.publisher;
    paper.abstract_text = req.abstract_text;
    if let Some(authors) = req.authors {
        paper.authors = authors;
    }

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
