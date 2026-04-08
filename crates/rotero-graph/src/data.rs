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

/// The kind of relationship that connects two papers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeType {
    /// Papers share a common tag.
    Tag,
    /// Papers belong to the same collection.
    Collection,
    /// Papers share a co-author.
    Author,
    /// Papers were published in the same journal.
    Journal,
}

/// A weighted, typed edge between two paper nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
    pub rel_type: EdgeType,
    pub label: String,
    pub weight: f32,
}

/// Complete graph output containing positioned nodes and relationship edges.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphData {
    pub nodes: Vec<GraphNode>,
    pub links: Vec<GraphEdge>,
}

/// Controls which edge types are included and caps edge density.
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
