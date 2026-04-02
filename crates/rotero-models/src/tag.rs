use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub id: Option<i64>,
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
