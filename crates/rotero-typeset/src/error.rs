//! Error type mapping Typst diagnostics into a readable form for the UI and agent.

use typst::diag::SourceDiagnostic;
use typst::ecow::EcoVec;

#[derive(Debug, thiserror::Error)]
pub enum TypesetError {
    /// One or more Typst source diagnostics (compile or export failure).
    #[error("typst compile failed:\n{0}")]
    Compile(String),
}

impl TypesetError {
    /// Join diagnostic messages into a single human-readable string.
    pub fn from_diagnostics(diags: EcoVec<SourceDiagnostic>) -> Self {
        let joined = diags
            .iter()
            .map(|d| d.message.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        TypesetError::Compile(joined)
    }
}
