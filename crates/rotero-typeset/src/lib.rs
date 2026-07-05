//! Compile authored documents to typeset PDF via an embedded Typst 0.15 compiler.
//!
//! Two authoring surfaces are supported (see [`DocumentFormat`]):
//!
//! - **Typst** — the body *is* Typst source, compiled directly. This is the
//!   real paper-authoring surface: full layout control, Universe templates that
//!   expect Typst input, native `#cite`/`#bibliography`.
//! - **Markdown** — the body is Markdown, wrapped and rendered through the
//!   `@preview/cmarker` package (CommonMark-in-Typst) with `@preview/mitex` for
//!   LaTeX math. This is the AI-native quick-summary surface.
//!
//! Both paths optionally apply a Typst Universe template and append a
//! Typst-native `#bibliography` (hayagriva under the hood) fed by a generated
//! BibTeX string.
//!
//! Typst's embedding API churns across minor versions, so all of it is confined
//! to this crate (see `world.rs`). Package versions are pinned below.

mod error;
mod world;

pub use error::TypesetError;

use world::RoteroWorld;

/// Pinned Typst Universe package versions. Bump deliberately.
const CMARKER: &str = "@preview/cmarker:0.1.10";
const MITEX: &str = "@preview/mitex:0.2.7";

/// The default template: plain Typst `article`-like output with no Universe package.
pub const DEFAULT_TEMPLATE: &str = "article";

/// Typst Universe package index (JSON list of all preview packages).
const UNIVERSE_INDEX: &str = "https://packages.typst.org/preview/index.json";

/// A Typst Universe template package usable as a document template.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TemplateInfo {
    pub name: String,
    pub version: String,
    pub description: String,
    /// Package categories (e.g. "paper", "report").
    pub categories: Vec<String>,
}

/// Fetch the list of available Typst Universe templates (packages that declare a
/// `[template]`). Network call; callers should cache the result. Returns the
/// latest version of each template package, sorted by name.
pub fn list_templates() -> Result<Vec<TemplateInfo>, TypesetError> {
    let body: serde_json::Value = ureq::get(UNIVERSE_INDEX)
        .call()
        .map_err(|e| TypesetError::Compile(format!("fetch universe index: {e}")))?
        .into_json()
        .map_err(|e| TypesetError::Compile(format!("parse universe index: {e}")))?;

    let arr = body.as_array().cloned().unwrap_or_default();
    // Keep only template packages, and only the newest version of each name.
    let mut latest: std::collections::HashMap<String, TemplateInfo> = std::collections::HashMap::new();
    for entry in arr {
        if entry.get("template").is_none() {
            continue;
        }
        let name = entry.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let version = entry.get("version").and_then(|v| v.as_str()).unwrap_or("");
        if name.is_empty() || version.is_empty() {
            continue;
        }
        let info = TemplateInfo {
            name: name.to_string(),
            version: version.to_string(),
            description: entry
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            categories: entry
                .get("categories")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|c| c.as_str().map(String::from)).collect())
                .unwrap_or_default(),
        };
        latest
            .entry(name.to_string())
            .and_modify(|existing| {
                if version_gt(&info.version, &existing.version) {
                    *existing = info.clone();
                }
            })
            .or_insert(info);
    }

    let mut out: Vec<TemplateInfo> = latest.into_values().collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// Compare two dotted semver-ish version strings; true if `a` > `b`.
fn version_gt(a: &str, b: &str) -> bool {
    let parse = |s: &str| -> Vec<u32> { s.split('.').filter_map(|p| p.parse().ok()).collect() };
    parse(a) > parse(b)
}

/// The authoring surface a document's body is written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DocumentFormat {
    /// The body is Typst source, compiled directly.
    #[default]
    Typst,
    /// The body is Markdown, rendered through `@preview/cmarker`.
    Markdown,
}

impl DocumentFormat {
    /// Stable string form (matches [`rotero_models`]'s stored value).
    pub fn as_str(self) -> &'static str {
        match self {
            DocumentFormat::Typst => "typst",
            DocumentFormat::Markdown => "markdown",
        }
    }

    /// Parse from the stored string, defaulting to Typst on unknown input.
    pub fn from_str_or_default(s: &str) -> Self {
        match s {
            "markdown" => DocumentFormat::Markdown,
            _ => DocumentFormat::Typst,
        }
    }
}

/// Options controlling a compile.
#[derive(Debug, Clone)]
pub struct CompileOptions {
    /// The surface the body is written in.
    pub format: DocumentFormat,
    /// Document title (rendered in the template header when supported).
    pub title: String,
    /// Author names.
    pub authors: Vec<String>,
    /// Template identifier: `"article"` (built-in default) or a Universe package
    /// name understood by [`template_import`]. Unknown names fall back to `article`.
    pub template: String,
    /// BibTeX source for the document's references. When present a `#bibliography`
    /// is appended and in-text `@key` / `#cite` markers in the body resolve.
    pub bib: Option<String>,
    /// CSL style name for the bibliography (e.g. `"apa"`, `"ieee"`, `"vancouver"`).
    pub csl_style: String,
}

impl Default for CompileOptions {
    fn default() -> Self {
        Self {
            format: DocumentFormat::default(),
            title: String::new(),
            authors: Vec::new(),
            template: DEFAULT_TEMPLATE.to_string(),
            bib: None,
            csl_style: "apa".to_string(),
        }
    }
}

/// Compile a document body to PDF bytes, dispatching on [`CompileOptions::format`].
///
/// A temporary project directory is created to host the generated `refs.bib`
/// (so `#bibliography` can resolve it) and to anchor Typst's file resolution;
/// it is removed when the returned bytes are produced.
pub fn compile(body: &str, opts: &CompileOptions) -> Result<Vec<u8>, TypesetError> {
    let project = tempdir().map_err(|e| TypesetError::Compile(format!("tempdir: {e}")))?;

    if let Some(bib) = &opts.bib {
        let bib_path = project.path().join("refs.bib");
        std::fs::write(&bib_path, bib)
            .map_err(|e| TypesetError::Compile(format!("write refs.bib: {e}")))?;
    }

    let main = match opts.format {
        DocumentFormat::Markdown => build_markdown_main(body, opts),
        DocumentFormat::Typst => build_typst_main(body, opts),
    };
    let world = RoteroWorld::new(&main, project.path().to_path_buf());
    world.compile_pdf()
}

/// Compile a Markdown body to PDF bytes.
///
/// Convenience wrapper over [`compile`] that forces [`DocumentFormat::Markdown`],
/// regardless of `opts.format`.
pub fn compile_markdown(md: &str, opts: &CompileOptions) -> Result<Vec<u8>, TypesetError> {
    let opts = CompileOptions {
        format: DocumentFormat::Markdown,
        ..opts.clone()
    };
    compile(md, &opts)
}

/// Build the synthetic Typst main file that wraps a Markdown body.
fn build_markdown_main(md: &str, opts: &CompileOptions) -> String {
    let mut out = String::new();

    // Imports: math renderer + markdown transpiler. cmarker calls the `math`
    // callback as `math(source, block: bool)`; mitex's `mitex(it, ..args)`
    // accepts the `block` named arg, whereas `mitext` does not.
    out.push_str(&format!("#import \"{MITEX}\": mitex\n"));
    out.push_str(&format!("#import \"{CMARKER}\"\n\n"));

    // Template application (Universe packages only; "article" == bare defaults).
    if let Some(import) = template_import(&opts.template) {
        out.push_str(&import);
        out.push('\n');
    }

    // Document title metadata for templates that read it.
    if !opts.title.is_empty() {
        out.push_str(&format!("#set document(title: {})\n", typst_str(&opts.title)));
    }

    // Render the Markdown body. LaTeX math ($...$) is handed to mitex.
    // In-text citations written as `[@key]` / `@key` are rewritten to cmarker's
    // raw-typst form so they become real Typst `#cite` calls (which resolve
    // against the appended `#bibliography`).
    let body = if opts.bib.is_some() {
        rewrite_citations(md)
    } else {
        md.to_string()
    };
    out.push_str("#cmarker.render(\n");
    out.push_str(&format!("  {},\n", typst_str(&body)));
    out.push_str("  math: mitex,\n");
    out.push_str(")\n\n");

    // Typst-native bibliography (hayagriva). In-text @key markers the author
    // included resolve here; the reference list renders regardless.
    if opts.bib.is_some() {
        out.push_str(&format!(
            "#bibliography(\"refs.bib\", style: {})\n",
            typst_str(&opts.csl_style)
        ));
    }

    out
}

/// Build the Typst main file for a body that is *already* Typst source.
///
/// The body is emitted verbatim, so it may use the full Typst language — its own
/// `#import`s, `#show` rules, `#figure`s, math, `@cite` refs, etc. We only
/// prepend a template `#show` (when the picker selected a Universe template and
/// the body hasn't applied its own) and a title, and append a `#bibliography`
/// when a `.bib` was supplied and the body didn't already declare one.
fn build_typst_main(src: &str, opts: &CompileOptions) -> String {
    let mut out = String::new();

    // Only apply the picker's template if the author hasn't taken layout into
    // their own hands with a `#show:` rule (respecting hand-written documents).
    let author_controls_layout = src.contains("#show:") || src.contains("#show :");
    if !author_controls_layout
        && let Some(import) = template_import(&opts.template)
    {
        out.push_str(&import);
        out.push('\n');
    }

    if !opts.title.is_empty() && !src.contains("#set document(") {
        out.push_str(&format!("#set document(title: {})\n", typst_str(&opts.title)));
    }

    if !out.is_empty() {
        out.push('\n');
    }
    out.push_str(src);

    // Append the collection bibliography unless the author placed their own.
    if opts.bib.is_some() && !src.contains("#bibliography(") {
        out.push_str(&format!(
            "\n\n#bibliography(\"refs.bib\", style: {})\n",
            typst_str(&opts.csl_style)
        ));
    }

    out
}

/// The `#import`/`#show` line applying a Universe template, or `None` for the
/// built-in `article` default (which needs no package).
fn template_import(template: &str) -> Option<String> {
    match template {
        DEFAULT_TEMPLATE | "" => None,
        // Known preprint template; more can be added as they're vetted.
        "arkheion" => {
            Some("#import \"@preview/arkheion:0.1.2\": arkheion\n#show: arkheion".to_string())
        }
        // A full "name:version" spec: import and apply its `template` show rule.
        other if other.contains(':') => {
            let name = other.split(':').next().unwrap_or(other);
            Some(format!("#import \"@preview/{other}\"\n#show: {name}.template"))
        }
        // Unknown bare name: fall back to defaults rather than emit a broken import.
        _ => None,
    }
}

/// Rewrite standard `[@key]` / `@key` citation syntax into cmarker `raw-typst`
/// spans holding real Typst `#cite(<key>)` calls.
///
/// Supported forms:
/// - `[@key]`            → parenthetical cite
/// - `[@key, p. 12]`     → cite with a page supplement
/// - `@key` (bare word)  → prose cite (author becomes part of the sentence)
///
/// A citation key is `[A-Za-z0-9_:-]+`. `@` not forming a key (e.g. an email)
/// is left untouched.
fn rewrite_citations(md: &str) -> String {
    let bytes = md.as_bytes();
    let mut out = String::with_capacity(md.len());
    let mut i = 0;
    while i < bytes.len() {
        // Bracketed form: [@key] or [@key, supplement]
        if bytes[i] == b'['
            && let Some((rendered, next)) = parse_bracket_cite(md, i)
        {
            out.push_str(&rendered);
            i = next;
            continue;
        }
        // Bare form: @key (must start a key char right after @, and @ must be at
        // a word boundary so we don't eat emails like a@b).
        if bytes[i] == b'@'
            && (i == 0 || !is_word_byte(bytes[i - 1]))
            && let Some((key, next)) = parse_key(md, i + 1)
        {
            out.push_str(&format!("<!--raw-typst #cite(<{key}>, form: \"prose\") -->"));
            i = next;
            continue;
        }
        // Default: copy this char.
        let ch_len = utf8_len(bytes[i]);
        out.push_str(&md[i..i + ch_len]);
        i += ch_len;
    }
    out
}

/// Parse `[@key]` or `[@key, supplement]` starting at `start` (the `[`).
/// Returns the rendered raw-typst span and the index just past the closing `]`.
fn parse_bracket_cite(md: &str, start: usize) -> Option<(String, usize)> {
    let bytes = md.as_bytes();
    // Require `[@` then a key.
    if bytes.get(start + 1) != Some(&b'@') {
        return None;
    }
    let (key, mut j) = parse_key(md, start + 2)?;
    // Optional `, supplement` before `]`.
    let mut supplement: Option<&str> = None;
    if bytes.get(j) == Some(&b',') {
        let sup_start = j + 1;
        // find closing ]
        let close = md[sup_start..].find(']')? + sup_start;
        supplement = Some(md[sup_start..close].trim());
        j = close;
    }
    if bytes.get(j) != Some(&b']') {
        return None;
    }
    j += 1; // consume ]
    let rendered = match supplement {
        Some(sup) if !sup.is_empty() => format!(
            "<!--raw-typst #cite(<{key}>, supplement: [{}]) -->",
            sup.replace(']', "")
        ),
        _ => format!("<!--raw-typst #cite(<{key}>) -->"),
    };
    Some((rendered, j))
}

/// Parse a citation key starting at `start`. Returns the key and the index past it.
fn parse_key(md: &str, start: usize) -> Option<(&str, usize)> {
    let bytes = md.as_bytes();
    let mut j = start;
    while j < bytes.len() && is_key_byte(bytes[j]) {
        j += 1;
    }
    if j == start {
        None
    } else {
        Some((&md[start..j], j))
    }
}

fn is_key_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b':' | b'-' | b'.')
}

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Length in bytes of the UTF-8 sequence whose leading byte is `b`.
fn utf8_len(b: u8) -> usize {
    match b {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        _ => 4,
    }
}

/// Escape an arbitrary string as a Typst string literal.
fn typst_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => {}
            '\t' => out.push_str("\\t"),
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

/// Create a uniquely-named temp directory (the `tempfile` crate is dev-only).
fn tempdir() -> std::io::Result<TempDir> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let base = std::env::temp_dir().join(format!("rotero-typeset-{}-{}", std::process::id(), n));
    std::fs::create_dir_all(&base)?;
    Ok(TempDir { path: base })
}

/// Minimal self-cleaning temp dir.
struct TempDir {
    path: std::path::PathBuf,
}

impl TempDir {
    fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_BIB: &str = r#"
@article{vaswani2017,
  title   = {Attention Is All You Need},
  author  = {Vaswani, Ashish and Shazeer, Noam and Parmar, Niki},
  journal = {Advances in Neural Information Processing Systems},
  year    = {2017}
}
@article{devlin2019,
  title   = {BERT: Pre-training of Deep Bidirectional Transformers},
  author  = {Devlin, Jacob and Chang, Ming-Wei},
  journal = {NAACL},
  year    = {2019}
}
"#;

    #[test]
    fn compiles_plain_markdown_to_pdf() {
        let md = "# Hello\n\nThis is **bold** and a formula $E = mc^2$.\n";
        let opts = CompileOptions {
            title: "Test Doc".to_string(),
            ..Default::default()
        };
        let pdf = compile_markdown(md, &opts).expect("compile should succeed");
        assert!(pdf.starts_with(b"%PDF"), "output should be a PDF");
        assert!(pdf.len() > 1000, "PDF should be non-trivial, got {}", pdf.len());
    }

    #[test]
    fn rewrite_citations_forms() {
        assert_eq!(
            rewrite_citations("see [@vaswani2017]."),
            "see <!--raw-typst #cite(<vaswani2017>) -->."
        );
        assert_eq!(
            rewrite_citations("see [@vaswani2017, p. 5]."),
            "see <!--raw-typst #cite(<vaswani2017>, supplement: [p. 5]) -->."
        );
        assert_eq!(
            rewrite_citations("As @vaswani2017 shows"),
            "As <!--raw-typst #cite(<vaswani2017>, form: \"prose\") --> shows"
        );
        // An email must be left alone.
        assert_eq!(rewrite_citations("mail a@b.com"), "mail a@b.com");
    }

    #[test]
    fn compiles_plain_typst_to_pdf() {
        let src = "= Hello\n\nThis is *bold* and a formula $E = m c^2$.\n";
        let opts = CompileOptions {
            format: DocumentFormat::Typst,
            title: "Test Doc".to_string(),
            ..Default::default()
        };
        let pdf = compile(src, &opts).expect("typst compile should succeed");
        assert!(pdf.starts_with(b"%PDF"), "output should be a PDF");
        assert!(pdf.len() > 1000, "PDF should be non-trivial, got {}", pdf.len());
    }

    #[test]
    fn typst_body_bibliography_appended_when_absent() {
        // A Typst body with a `@cite` but no explicit `#bibliography` gets one
        // appended from the supplied `.bib`.
        let src = "= Survey\n\nTransformers @vaswani2017 led to BERT @devlin2019.\n";
        let opts = CompileOptions {
            format: DocumentFormat::Typst,
            title: "Survey".to_string(),
            bib: Some(SAMPLE_BIB.to_string()),
            csl_style: "ieee".to_string(),
            ..Default::default()
        };
        let pdf = compile(src, &opts).expect("typst compile with bib should succeed");
        assert!(pdf.starts_with(b"%PDF"));
        assert!(pdf.len() > 1000);
    }

    #[test]
    fn compiles_with_bibliography_numeric_and_author_date() {
        let md = "# Survey\n\nTransformers [@vaswani2017] led to BERT [@devlin2019].\n";
        for style in ["ieee", "apa"] {
            let opts = CompileOptions {
                title: "Survey".to_string(),
                bib: Some(SAMPLE_BIB.to_string()),
                csl_style: style.to_string(),
                ..Default::default()
            };
            let pdf = compile_markdown(md, &opts)
                .unwrap_or_else(|e| panic!("compile with style {style} failed: {e}"));
            assert!(pdf.starts_with(b"%PDF"), "style {style} should yield a PDF");
            assert!(pdf.len() > 1000, "style {style} PDF too small");
        }
    }
}
