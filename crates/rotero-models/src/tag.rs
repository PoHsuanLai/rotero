use serde::{Deserialize, Serialize};

/// A user-defined label that can be applied to one or more papers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tag {
    pub id: Option<String>,
    pub name: String,
    pub color: Option<String>,
}

impl Tag {
    /// Create a new tag with the given name and no color.
    pub fn new(name: String) -> Self {
        Self {
            id: None,
            name,
            color: None,
        }
    }
}
