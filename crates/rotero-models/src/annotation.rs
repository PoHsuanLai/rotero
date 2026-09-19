use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// The kind of annotation placed on a PDF page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnnotationType {
    Highlight,
    Note,
    Area,
    Underline,
    StrikeOut,
    Squiggly,
    Ink,
    Text,
}

impl AnnotationType {
    /// Storage form written to SQLite (`highlight`, `strikeout`, …).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Highlight => "highlight",
            Self::Note => "note",
            Self::Area => "area",
            Self::Underline => "underline",
            Self::StrikeOut => "strikeout",
            Self::Squiggly => "squiggly",
            Self::Ink => "ink",
            Self::Text => "text",
        }
    }

    /// Short UI label (`Highlight`, `StrikeOut`, …).
    pub fn label(self) -> &'static str {
        match self {
            Self::Highlight => "Highlight",
            Self::Note => "Note",
            Self::Area => "Area",
            Self::Underline => "Underline",
            Self::StrikeOut => "StrikeOut",
            Self::Squiggly => "Squiggly",
            Self::Ink => "Ink",
            Self::Text => "Text",
        }
    }

    /// Parse a stored type string, including a few historical aliases.
    pub fn parse(s: &str) -> Self {
        match s {
            "highlight" => Self::Highlight,
            "note" => Self::Note,
            "area" => Self::Area,
            "underline" => Self::Underline,
            "strikeout" | "strike_out" | "strike-out" => Self::StrikeOut,
            "squiggly" => Self::Squiggly,
            "ink" => Self::Ink,
            "text" => Self::Text,
            _ => Self::Note,
        }
    }
}

/// A user annotation on a specific page of a paper's PDF.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
