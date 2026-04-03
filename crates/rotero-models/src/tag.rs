use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tag {
    pub id: Option<String>,
    pub name: String,
    pub color: Option<String>,
}

impl Tag {
    pub fn new(name: String) -> Self {
        Self {
            id: None,
            name,
            color: None,
        }
    }
}
