use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Collection {
    pub id: Option<String>,
    pub name: String,
    pub parent_id: Option<String>,
    pub position: i32,
}

impl Collection {
    pub fn new(name: String) -> Self {
        Self {
            id: None,
            name,
            parent_id: None,
            position: 0,
        }
    }
}
