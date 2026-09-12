//! Embedded file attachments from the document catalog.

use pdfrum::Document;
use serde::{Deserialize, Serialize};

/// One catalog-level embedded file (`/Names /EmbeddedFiles`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddedAttachment {
    /// Name-tree key.
    pub name: String,
    /// Suggested file name.
    pub file_name: String,
    pub description: String,
    /// MIME subtype when present.
    pub subtype: Option<String>,
    /// Byte length when the stream is available.
    pub size: Option<usize>,
}

/// List document-level embedded files (no payload).
pub fn list_attachments(doc: &Document) -> Vec<EmbeddedAttachment> {
    doc.attachments()
        .into_iter()
        .map(|a| {
            let data = a.data();
            EmbeddedAttachment {
                name: a.name.clone(),
                file_name: a.file_name(),
                description: a.description(),
                subtype: a.subtype().filter(|s| !s.is_empty()),
                size: data.as_ref().map(|b| b.len()),
            }
        })
        .collect()
}
