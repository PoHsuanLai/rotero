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
    pub collection_id: Option<i64>,
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

pub async fn save_paper(
    State(state): State<Arc<ConnectorState>>,
    Json(req): Json<SavePaperRequest>,
) -> Json<SavePaperResponse> {
    let title = req.title.unwrap_or_else(|| "Untitled".to_string());
    let mut paper = rotero_models::Paper::new(title);
    paper.doi = req.doi;
    paper.url = req.url;
    if let Some(authors) = req.authors {
        paper.authors = authors;
    }

    if let Some(ref callback) = state.on_paper_saved {
        callback(paper.clone(), req.collection_id);
    }

    Json(SavePaperResponse {
        success: true,
        message: "Paper added to library".to_string(),
        paper_id: paper.id,
    })
}
