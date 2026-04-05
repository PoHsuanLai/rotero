use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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
