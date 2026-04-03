use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnnotationType {
    Highlight,
    Note,
    Area,
    Underline,
    Ink,
    Text,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Annotation {
    pub id: Option<i64>,
    pub paper_id: i64,
    pub page: i32,
    pub ann_type: AnnotationType,
    pub color: String,
    pub content: Option<String>,
    pub geometry: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
}
