use serde::{Deserialize, Serialize};

/// A named folder for organizing papers, supporting nested hierarchies.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Collection {
    pub id: Option<String>,
    pub name: String,
    pub parent_id: Option<String>,
    pub position: i32,
}

impl Collection {
    /// Create a new root-level collection with the given name.
    pub fn new(name: String) -> Self {
        Self {
            id: None,
            name,
            parent_id: None,
            position: 0,
        }
    }
}
