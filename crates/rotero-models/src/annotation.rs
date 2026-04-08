use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// The kind of annotation placed on a PDF page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnnotationType {
    Highlight,
    Note,
    Area,
    Underline,
    Ink,
    Text,
}

/// A user annotation on a specific page of a paper's PDF.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Annotation {
    pub id: Option<String>,
    pub paper_id: String,
    pub page: i32,
    pub ann_type: AnnotationType,
    pub color: String,
    pub content: Option<String>,
    /// Position and dimensions as JSON (format depends on annotation type).
    pub geometry: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
}
