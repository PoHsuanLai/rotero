use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// What kind of document this is — drives the agent prompt and default template.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DocumentKind {
    /// A prose summary of a collection's papers.
    Summary,
    /// A step-by-step tutorial synthesizing a collection.
    Tutorial,
    /// A research report (library- or web-grounded).
    Research,
    /// A full authored manuscript / paper.
    Manuscript,
    /// A structured literature review.
    LitReview,
}

impl DocumentKind {
    /// Stable string form stored in the database.
    pub fn as_str(self) -> &'static str {
        match self {
            DocumentKind::Summary => "summary",
            DocumentKind::Tutorial => "tutorial",
            DocumentKind::Research => "research",
            DocumentKind::Manuscript => "manuscript",
            DocumentKind::LitReview => "lit_review",
        }
    }

    /// Parse from the stored string, defaulting to `Summary` on unknown input.
    pub fn from_str_or_default(s: &str) -> Self {
        match s {
            "tutorial" => DocumentKind::Tutorial,
            "research" => DocumentKind::Research,
            "manuscript" => DocumentKind::Manuscript,
            "lit_review" => DocumentKind::LitReview,
            _ => DocumentKind::Summary,
        }
    }
}

/// A standalone authored document (summary, report, or paper), optionally linked
/// to a collection whose papers supply its references. Unlike [`crate::Note`],
/// it is not owned by a single paper.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub id: Option<String>,
    pub title: String,
    /// Markdown body (authored by the user and/or the agent).
    pub body: String,
    /// Collection this document draws its papers/citations from, if any.
    pub collection_id: Option<String>,
    /// Typst template name (`"article"` default, or a Universe package).
    pub template: String,
    /// CSL bibliography style (e.g. `"apa"`, `"ieee"`).
    pub csl_style: String,
    pub kind: DocumentKind,
    /// Path to the most recently compiled PDF on disk, if compiled.
    pub last_pdf_path: Option<String>,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
}

impl Document {
    /// Create a new empty document of the given kind.
    pub fn new(title: String, kind: DocumentKind, collection_id: Option<String>) -> Self {
        let now = Utc::now();
        Self {
            id: None,
            title,
            body: String::new(),
            collection_id,
            template: "article".to_string(),
            csl_style: "apa".to_string(),
            kind,
            last_pdf_path: None,
            created_at: now,
            modified_at: now,
        }
    }
}
