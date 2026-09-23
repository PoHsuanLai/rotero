use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// What kind of thing a concept page names.
///
/// These are the entities a bibliography does not already store. Authors and
/// venues stay on the paper.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConceptKind {
    Method,
    Dataset,
    Benchmark,
    Task,
    Idea,
}

impl ConceptKind {
    /// Storage form (`method`, `dataset`, …).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Method => "method",
            Self::Dataset => "dataset",
            Self::Benchmark => "benchmark",
            Self::Task => "task",
            Self::Idea => "idea",
        }
    }

    /// Parse a stored or tool-supplied kind. Unknown strings are rejected.
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "method" => Self::Method,
            "dataset" => Self::Dataset,
            "benchmark" => Self::Benchmark,
            "task" => Self::Task,
            "idea" => Self::Idea,
            _ => return None,
        })
    }
}

/// A maintained page for a method, dataset, benchmark, task, or idea.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Concept {
    pub id: Option<String>,
    pub kind: ConceptKind,
    pub title: String,
    pub slug: String,
    pub body: String,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
}

/// A concept plus the claims and sibling concepts that point at it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConceptPage {
    pub concept: Concept,
    pub claims: Vec<super::Claim>,
    pub related: Vec<Concept>,
}
