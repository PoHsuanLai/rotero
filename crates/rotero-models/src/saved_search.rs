use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A persisted search query that can be re-run from the sidebar.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedSearch {
    pub id: Option<String>,
    pub name: String,
    pub query: String,
    pub created_at: DateTime<Utc>,
}

impl SavedSearch {
    /// Create a new saved search with the current timestamp.
    pub fn new(name: String, query: String) -> Self {
        Self {
            id: None,
            name,
            query,
            created_at: Utc::now(),
        }
    }
}
