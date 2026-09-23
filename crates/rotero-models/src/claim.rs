use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// How much trust a claim carries.
///
/// `from_chat` is an answer the agent filed without a quotation. Search ranks
/// it below a claim that quotes the paper, and a later `from_chat` write must
/// not replace one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimStatus {
    Confirmed,
    Extracted,
    FromChat,
}

impl ClaimStatus {
    /// Storage form (`confirmed`, `extracted`, `from_chat`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Confirmed => "confirmed",
            Self::Extracted => "extracted",
            Self::FromChat => "from_chat",
        }
    }

    /// Parse a stored or tool-supplied status.
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "confirmed" => Self::Confirmed,
            "extracted" => Self::Extracted,
            "from_chat" => Self::FromChat,
            _ => return None,
        })
    }

    /// A quotation is required for every status except `from_chat`.
    pub fn requires_quote(self) -> bool {
        !matches!(self, Self::FromChat)
    }

    /// Sort key for search: confirmed, then extracted, then from_chat.
    pub fn rank(self) -> u8 {
        match self {
            Self::Confirmed => 0,
            Self::Extracted => 1,
            Self::FromChat => 2,
        }
    }
}

/// One sentence a paper states, with the quotation copied onto the row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Claim {
    pub id: Option<String>,
    pub paper_id: String,
    pub statement: String,
    pub quote: String,
    pub page: Option<i32>,
    pub annotation_id: Option<String>,
    pub status: ClaimStatus,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
}

/// The fields a caller supplies when filing a claim.
///
/// Identity is the paper plus the normalized statement. The row's id is
/// chosen by the database.
#[derive(Debug, Clone, PartialEq)]
pub struct ClaimDraft {
    pub paper_id: String,
    pub statement: String,
    pub quote: String,
    pub page: Option<i32>,
    pub annotation_id: Option<String>,
    pub status: ClaimStatus,
}

/// A citation target the library does not own yet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReferenceStub {
    pub id: String,
    pub citing_paper_id: String,
    pub identifier: String,
    pub raw: String,
    pub created_at: DateTime<Utc>,
}

/// What recording one external PDF link did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CitationRecord {
    /// The link resolved to a paper already in the library.
    Citation { cited_id: String },
    /// The link carried an identifier the library does not own.
    Stub { id: String },
    /// A self-link, or a URI with no DOI, arXiv id, PMID, or ISBN.
    Ignored,
}
