use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A free-form text note attached to a paper.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    pub id: Option<String>,
    pub paper_id: String,
    pub title: String,
    pub body: String,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
}

impl Note {
    /// Create a new note for the given paper with an empty body.
    pub fn new(paper_id: String, title: String) -> Self {
        let now = Utc::now();
        Self {
            id: None,
            paper_id,
            title,
            body: String::new(),
            created_at: now,
            modified_at: now,
        }
    }
}
