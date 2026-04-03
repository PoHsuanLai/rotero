use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedSearch {
    pub id: Option<i64>,
    pub name: String,
    pub query: String,
    pub created_at: DateTime<Utc>,
}

impl SavedSearch {
    pub fn new(name: String, query: String) -> Self {
        Self {
            id: None,
            name,
            query,
            created_at: Utc::now(),
        }
    }
}
