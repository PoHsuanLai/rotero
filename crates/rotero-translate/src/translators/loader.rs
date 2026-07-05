//! Loads the upstream Zotero translator corpus (`zotero/translators`) from disk
//! into [`JsTranslator`]s at registry init.
//!
//! The corpus is vendored as a git submodule under
//! `crates/rotero-translate/vendor/translators`. `git submodule update --remote`
//! tracks upstream, so translator fixes land for all sites without editing Rust.
//! Loading from disk (rather than embedding via `include_dir!`) keeps the binary
//! lean — the corpus is ~13 MB — at the cost of needing the directory present at
//! runtime; the desktop app ships it alongside the binary.

use std::path::{Path, PathBuf};

use super::JsTranslator;

/// Directory holding the translator `.js` files, relative to the crate.
const VENDOR_SUBDIR: &str = "vendor/translators";

/// Resolve the translator corpus directory. Checks, in order:
/// 1. `ROTERO_TRANSLATORS_DIR` (explicit override, e.g. for the packaged app),
/// 2. the vendored submodule next to the crate source (`CARGO_MANIFEST_DIR`),
///    which covers tests and dev runs.
///
/// Returns `None` if neither exists, so the registry simply loads no JS
/// translators and runs with only the built-in Rust hubs.
pub fn corpus_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("ROTERO_TRANSLATORS_DIR") {
        let p = PathBuf::from(dir);
        if p.is_dir() {
            return Some(p);
        }
    }
    let vendored = Path::new(env!("CARGO_MANIFEST_DIR")).join(VENDOR_SUBDIR);
    vendored.is_dir().then_some(vendored)
}

/// Load every translator `.js` in `dir` into a [`JsTranslator`], skipping files
/// whose header doesn't parse or that carry no `target` (import/export/search
/// translators, which aren't web-dispatchable). Returns them unsorted; the
/// registry orders by priority.
pub fn load_from_dir(dir: &Path) -> Vec<JsTranslator> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!("translator corpus unreadable at {}: {e}", dir.display());
            return Vec::new();
        }
    };

    let mut loaded = Vec::new();
    let mut skipped = 0usize;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("js") {
            continue;
        }
        let source = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                tracing::debug!("skip {}: {e}", path.display());
                skipped += 1;
                continue;
            }
        };
        match JsTranslator::from_source(&source) {
            // A translator with no usable target can never be dispatched by URL,
            // so drop it here rather than carry dead weight in the registry.
            Ok(t) if t.has_target() => loaded.push(t),
            Ok(_) => skipped += 1,
            Err(e) => {
                tracing::debug!("skip {}: {e}", path.display());
                skipped += 1;
            }
        }
    }
    tracing::info!(
        "loaded {} JS translators from {} ({skipped} skipped)",
        loaded.len(),
        dir.display()
    );
    loaded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_the_vendored_corpus() {
        // In dev/CI the submodule is checked out; if not, this is a no-op pass.
        let Some(dir) = corpus_dir() else {
            eprintln!("no corpus dir; skipping");
            return;
        };
        let total_js = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("js"))
            .count();
        let translators = load_from_dir(&dir);
        eprintln!(
            "corpus: {} loaded / {} .js files (target-less + regex-rejected skipped)",
            translators.len(),
            total_js
        );
        assert!(
            translators.len() > 300,
            "expected the bulk of the corpus to load, got {}",
            translators.len()
        );
    }
}
