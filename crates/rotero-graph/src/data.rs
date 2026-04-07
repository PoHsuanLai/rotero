use serde::{Deserialize, Serialize};

/// A node in the paper graph (one per paper).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    pub x: f64,
    pub y: f64,
    pub size: f64,
    pub color: String,
    pub is_read: bool,
    pub is_favorite: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeType {
    Tag,
    Collection,
    Author,
    Journal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
    pub rel_type: EdgeType,
    pub label: String,
    pub weight: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphData {
    pub nodes: Vec<GraphNode>,
    pub links: Vec<GraphEdge>,
}

#[derive(Debug, Clone)]
pub struct GraphFilter {
    pub show_tag_edges: bool,
    pub show_collection_edges: bool,
    pub show_author_edges: bool,
    pub show_journal_edges: bool,
    pub max_edges_per_node: usize,
    pub max_author_group_size: usize,
}

impl Default for GraphFilter {
    fn default() -> Self {
        Self {
            show_tag_edges: true,
            show_collection_edges: true,
            show_author_edges: true,
            show_journal_edges: false,
            max_edges_per_node: 15,
            max_author_group_size: 20,
        }
    }
}
