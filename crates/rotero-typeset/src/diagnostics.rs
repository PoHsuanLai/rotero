//! Editor diagnostics: compile a document and surface Typst's errors and
//! warnings as byte ranges into the *authored body* (not the wrapped main file),
//! ready to feed CodeMirror's lint gutter.

use serde::Serialize;
use typst::WorldExt;
use typst::diag::{Severity, SourceDiagnostic, Warned};
use typst::ecow::EcoVec;

/// A single diagnostic positioned in the authored body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Diagnostic {
    /// Byte offset into the body where the diagnostic starts.
    pub from: usize,
    /// Byte offset into the body where the diagnostic ends (>= `from`).
    pub to: usize,
    /// `"error"` or `"warning"` — matches CodeMirror's lint severities.
    pub severity: String,
    /// Human-readable message (plus any hints, appended).
    pub message: String,
}

/// The severity string CodeMirror's lint interface expects.
fn severity_str(s: Severity) -> &'static str {
    match s {
        Severity::Error => "error",
        Severity::Warning => "warning",
    }
}

/// Convert one Typst diagnostic into an editor diagnostic, mapping its span to a
/// byte range in the *body*.
///
/// `world` resolves the span to a byte range in the wrapped main source;
/// `prologue_len` is the byte length of the wrapper we prepend, subtracted so
/// ranges line up with the body the user actually edits. Diagnostics that fall
/// inside the prologue, or point into another file (an imported package), are
/// dropped — they aren't actionable in this editor.
fn to_body_diagnostic(
    world: &crate::world::RoteroWorld,
    body_len: usize,
    prologue_len: usize,
    diag: &SourceDiagnostic,
) -> Option<Diagnostic> {
    let mut message = diag.message.to_string();
    for hint in &diag.hints {
        message.push_str("\nhint: ");
        message.push_str(hint.v.as_str());
    }

    // The Markdown path embeds the body as an escaped string literal, so spans
    // don't map back to the raw body. Surface such diagnostics at the document
    // start rather than dropping them.
    if prologue_len == usize::MAX {
        return Some(Diagnostic {
            from: 0,
            to: 0,
            severity: severity_str(diag.severity).to_string(),
            message,
        });
    }

    // Only diagnostics pointing at the main file map into the body.
    let main_id = world.main_id();
    if diag.span.id() != Some(main_id) {
        return None;
    }
    let range = world.range(diag.span)?;

    // Shift from wrapped-main coordinates into body coordinates, discarding
    // anything in the prepended prologue.
    let from = range.start.checked_sub(prologue_len)?;
    let to = range.end.checked_sub(prologue_len)?;
    if from > body_len {
        return None;
    }
    let to = to.min(body_len);

    Some(Diagnostic {
        from,
        to,
        severity: severity_str(diag.severity).to_string(),
        message,
    })
}

/// Compile `body` under `opts` and return all errors and warnings as body-space
/// diagnostics. An empty vec means the document compiles cleanly.
///
/// This runs a full compile (syntax + semantic + layout), so it catches
/// everything Typst can report; callers should debounce it off the edit path.
pub fn diagnostics(body: &str, opts: &crate::CompileOptions) -> Vec<Diagnostic> {
    // Reuse the exact wrapping the real compile uses, and measure the prologue so
    // spans can be shifted back into body coordinates.
    let (main, prologue_len) = crate::build_main_with_prologue(body, opts);

    // A temp project dir hosts refs.bib (if any) exactly as `compile` does.
    let project = match crate::tempdir() {
        Ok(p) => p,
        Err(_) => return Vec::new(),
    };
    if let Some(bib) = &opts.bib {
        let _ = std::fs::write(project.path().join("refs.bib"), bib);
    }

    let world = crate::world::RoteroWorld::new(&main, project.path().to_path_buf());
    let Warned { output, warnings } =
        typst::compile::<typst_layout::PagedDocument>(&world);

    let errors: EcoVec<SourceDiagnostic> = match output {
        Ok(_) => EcoVec::new(),
        Err(e) => e,
    };

    let body_len = body.len();
    errors
        .iter()
        .chain(warnings.iter())
        .filter_map(|d| to_body_diagnostic(&world, body_len, prologue_len, d))
        .collect()
}
