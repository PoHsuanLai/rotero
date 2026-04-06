//! All parameter structs for MCP tool calls.

use rmcp::schemars;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, schemars::JsonSchema)]
pub struct SearchPapersParams {
    /// Search query string (searches title, authors, abstract, full text)
    pub query: String,
    /// Maximum number of results (default 20, max 50)
    pub limit: Option<u32>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct GetPaperParams {
    /// Paper ID
    pub paper_id: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ListPapersParams {
    /// Offset for pagination (default 0)
    pub offset: Option<u32>,
    /// Number of papers to return (default 20, max 100)
    pub limit: Option<u32>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct PaperIdParams {
    /// Paper ID
    pub paper_id: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct CollectionIdParams {
    /// Collection ID
    pub collection_id: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct TagIdParams {
    /// Tag ID
    pub tag_id: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ExtractPdfTextParams {
    /// Paper ID
    pub paper_id: String,
    /// Page numbers to extract (0-indexed). If omitted, extracts first 10 pages.
    pub pages: Option<Vec<u32>>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct AddNoteParams {
    /// Paper ID to add note to
    pub paper_id: String,
    /// Note title
    pub title: String,
    /// Note body text
    pub body: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct UpdateNoteParams {
    /// Note ID to update
    pub note_id: String,
    /// New title
    pub title: String,
    /// New body text
    pub body: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct AddTagToPaperParams {
    /// Paper ID
    pub paper_id: String,
    /// Tag name (will be created if it doesn't exist)
    pub tag_name: String,
    /// Optional tag color (hex, e.g. "#ff0000")
    pub color: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct SetPaperReadParams {
    /// Paper ID
    pub paper_id: String,
    /// Whether the paper is read
    pub is_read: bool,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct SetPaperFavoriteParams {
    /// Paper ID
    pub paper_id: String,
    /// Whether the paper is a favorite
    pub is_favorite: bool,
}

#[derive(Serialize)]
pub(super) struct LibraryStats {
    pub total_papers: u32,
    pub total_collections: u32,
    pub total_tags: u32,
    pub unread_count: u32,
    pub favorites_count: u32,
}
