//! All parameter structs for MCP tool calls.

use rmcp::schemars;
use serde::{Deserialize, Serialize};

/// Parameters for the `search_papers` tool.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct SearchPapersParams {
    /// Search query string (searches title, authors, abstract, full text)
    pub query: String,
    /// Maximum number of results (default 20, max 50)
    pub limit: Option<u32>,
}

/// Parameters for the `get_paper` tool.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct GetPaperParams {
    pub paper_id: String,
}

/// Parameters for the `list_papers` tool.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct ListPapersParams {
    /// Offset for pagination (default 0)
    pub offset: Option<u32>,
    /// Number of papers to return (default 20, max 100)
    pub limit: Option<u32>,
}

/// Parameters for tools that require only a paper ID.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct PaperIdParams {
    pub paper_id: String,
}

/// Parameters for tools that require only a collection ID.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct CollectionIdParams {
    pub collection_id: String,
}

/// Parameters for tools that require only a tag ID.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct TagIdParams {
    pub tag_id: String,
}

/// Parameters for the `extract_pdf_text` tool with optional page range.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct ExtractPdfTextParams {
    pub paper_id: String,
    /// First page to extract (1-indexed, default 1)
    pub page_start: Option<u32>,
    /// Last page to extract (1-indexed, inclusive, default page_start + 9)
    pub page_end: Option<u32>,
}

#[derive(Serialize)]
pub(super) struct ExtractPdfTextResult {
    /// The extracted text for the requested page range
    pub text: String,
    /// First page returned (1-indexed)
    pub page_start: u32,
    /// Last page returned (1-indexed)
    pub page_end: u32,
    /// Total number of pages in the PDF
    pub total_pages: u32,
}

/// Parameters for the `add_note` tool.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct AddNoteParams {
    pub paper_id: String,
    pub title: String,
    pub body: String,
}

/// Parameters for the `update_note` tool.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct UpdateNoteParams {
    pub note_id: String,
    pub title: String,
    pub body: String,
}

/// Parameters for the `add_tag_to_paper` tool.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct AddTagToPaperParams {
    pub paper_id: String,
    /// Tag name (will be created if it doesn't exist)
    pub tag_name: String,
    /// Optional tag color (hex, e.g. "#ff0000")
    pub color: Option<String>,
}

/// Parameters for the `set_paper_read` tool.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct SetPaperReadParams {
    pub paper_id: String,
    pub is_read: bool,
}

/// Parameters for the `set_paper_favorite` tool.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct SetPaperFavoriteParams {
    pub paper_id: String,
    pub is_favorite: bool,
}

/// Parameters for the `get_paper_relationships` tool.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct GetPaperRelationshipsParams {
    pub paper_id: String,
}

/// Parameters for the `get_library_graph` tool.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct GetLibraryGraphParams {
    /// Maximum number of edges to return (default 100, max 500)
    pub max_edges: Option<u32>,
}

#[derive(Serialize)]
pub(super) struct PaperRelationship {
    pub related_paper_id: String,
    pub related_paper_title: String,
    pub relationship_type: String,
    pub label: String,
    pub weight: f32,
}

#[derive(Serialize)]
pub(super) struct GraphNode {
    pub id: String,
    pub title: String,
    pub authors: Vec<String>,
    pub year: Option<i32>,
}

#[derive(Serialize)]
pub(super) struct GraphEdge {
    pub source: String,
    pub target: String,
    pub relationship_type: String,
    pub label: String,
    pub weight: f32,
}

#[derive(Serialize)]
pub(super) struct LibraryGraph {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

#[derive(Serialize)]
pub(super) struct LibraryStats {
    pub total_papers: u32,
    pub total_collections: u32,
    pub total_tags: u32,
    pub unread_count: u32,
    pub favorites_count: u32,
}
