use serde::{Deserialize, Serialize};

use crate::{Claim, Concept, Paper};

/// A typed link between a claim and a concept, two claims, or two concepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WikiRel {
    About,
    Supports,
    Qualifies,
    Disputes,
    Related,
}

impl WikiRel {
    /// Storage form.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::About => "about",
            Self::Supports => "supports",
            Self::Qualifies => "qualifies",
            Self::Disputes => "disputes",
            Self::Related => "related",
        }
    }

    /// Parse a tool-supplied relation. Unknown strings are rejected.
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "about" => Self::About,
            "supports" => Self::Supports,
            "qualifies" => Self::Qualifies,
            "disputes" => Self::Disputes,
            "related" => Self::Related,
            _ => return None,
        })
    }
}

/// Endpoint kind stored on `wiki_edges`.
pub const ENDPOINT_CLAIM: &str = "claim";
/// Endpoint kind stored on `wiki_edges`.
pub const ENDPOINT_CONCEPT: &str = "concept";

/// Concepts, claims, and papers matching one query.
///
/// Claims are ordered confirmed, then extracted, then `from_chat`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WikiSearch {
    pub concepts: Vec<Concept>,
    pub claims: Vec<Claim>,
    pub papers: Vec<Paper>,
}

/// Instructions for the agent that compiles one paper into the wiki.
///
/// The same text is the MCP prompt and the in-app **Add to wiki** request, so
/// the two cannot drift.
pub fn compile_paper_prompt(paper_id: &str, title: &str) -> String {
    format!(
        "\
Add this paper to the wiki. Paper ID: {paper_id}. Title: {title}.

1. Call list_concepts and list_claims for this paper, and list_cited and list_citing.
2. Read the abstract and the highlights. Read PDF pages only when the abstract is empty and there are no highlights, or the user asked for a full pass.
3. upsert_concept for methods, datasets, benchmarks, tasks, and ideas that are actually in the source. Reuse a catalog hit instead of minting a near-duplicate title.
4. file_claim once per sentence, with the quote and page, status extracted, then link_claim_concept.
5. When a new claim plainly agrees with, narrows, or conflicts with a claim already listed on a neighbor, link_claims. Skip the pair when that is not obvious.
6. Leave notes, annotations, and paper metadata alone. Do not file a chat answer as extracted. A claim needs the quotation it came from.

Do not say the library has nothing on a topic until list_concepts and search_wiki have both come back empty.\n"
    )
}
