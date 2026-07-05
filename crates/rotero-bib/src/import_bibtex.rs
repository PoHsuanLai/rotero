use biblatex::{Bibliography, Chunk, ChunksExt, PermissiveType, Person, Spanned};
use rotero_models::{Paper, PaperLinks, Publication};

/// A paper parsed from BibTeX, with an optional linked PDF path.
pub struct ImportedPaper {
    /// The parsed paper metadata.
    pub paper: Paper,
    /// Path to a PDF extracted from the BibTeX `file` field, if present.
    pub source_pdf: Option<String>,
}

/// Parses a BibTeX string and returns the extracted papers.
///
/// The whole file is parsed as one bibliography. When that fails (a single
/// malformed entry otherwise rejects the entire file, or a `@preamble` block the
/// backend cannot digest), the input is split into individual entries and each is
/// parsed on its own, so the well-formed entries are still recovered.
pub fn import_bibtex(input: &str) -> Result<Vec<ImportedPaper>, String> {
    let bibliography = match Bibliography::parse(input) {
        Ok(bib) => bib,
        Err(_) => return parse_tolerant(input),
    };

    Ok(bibliography.iter().map(entry_to_paper).collect())
}

/// Parse each top-level `@…{…}` entry independently, dropping any that the backend
/// rejects. Recovers the good entries from a file whose overall parse failed.
fn parse_tolerant(input: &str) -> Result<Vec<ImportedPaper>, String> {
    let mut papers = Vec::new();

    for block in split_entries(input) {
        let Ok(bib) = Bibliography::parse(&block) else {
            continue;
        };
        papers.extend(bib.iter().map(entry_to_paper));
    }

    if papers.is_empty() {
        return Err("Failed to parse BibTeX: no recoverable entries".to_string());
    }
    Ok(papers)
}

/// Split a BibTeX source into its top-level `@type{…}` / `@type(…)` blocks,
/// tracking brace/paren nesting so entry bodies aren't cut mid-field. Non-entry
/// declarations (`@preamble`, `@comment`, `@string`) the backend can't resolve in
/// isolation are skipped.
fn split_entries(input: &str) -> Vec<String> {
    let mut entries = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] != '@' {
            i += 1;
            continue;
        }

        // Read the entry type keyword following the `@`.
        let type_start = i + 1;
        let mut j = type_start;
        while j < chars.len() && (chars[j].is_alphanumeric() || chars[j] == '_') {
            j += 1;
        }
        let entry_type: String = chars[type_start..j]
            .iter()
            .collect::<String>()
            .to_lowercase();

        // Skip whitespace to the opening delimiter.
        let mut k = j;
        while k < chars.len() && chars[k].is_whitespace() {
            k += 1;
        }
        let open = chars.get(k).copied();
        let (open_delim, close_delim) = match open {
            Some('{') => ('{', '}'),
            Some('(') => ('(', ')'),
            _ => {
                i = j.max(i + 1);
                continue;
            }
        };

        // Walk to the matching close delimiter, respecting nested braces.
        let body_start = k;
        let mut depth = 0usize;
        let mut end = body_start;
        while end < chars.len() {
            let c = chars[end];
            if c == open_delim {
                depth += 1;
            } else if c == close_delim {
                depth -= 1;
                if depth == 0 {
                    end += 1;
                    break;
                }
            }
            end += 1;
        }

        if !matches!(entry_type.as_str(), "preamble" | "comment" | "string") {
            entries.push(chars[i..end].iter().collect());
        }
        i = end;
    }

    entries
}

/// Convert one parsed bibliography entry into an [`ImportedPaper`].
fn entry_to_paper(entry: &biblatex::Entry) -> ImportedPaper {
    let title = entry.title().map(decode_field).unwrap_or_default();

    let authors: Vec<String> = entry
        .author()
        .unwrap_or_default()
        .iter()
        .map(person_display)
        .collect();

    let year = entry.date().ok().and_then(|d| match d {
        PermissiveType::Typed(date) => {
            let datetime = match date.value {
                biblatex::DateValue::At(dt)
                | biblatex::DateValue::After(dt)
                | biblatex::DateValue::Before(dt) => dt,
                biblatex::DateValue::Between(dt, _) => dt,
            };
            Some(datetime.year)
        }
        PermissiveType::Chunks(chunks) => {
            let s = chunks.format_verbatim();
            s.split('-').next().and_then(|y| y.parse::<i32>().ok())
        }
    });

    // Prefer the full journal name from the non-standard `fjournal` field over the
    // abbreviation typically stored in `journal`.
    let journal = entry
        .get("fjournal")
        .or_else(|| entry.journal().ok())
        .map(decode_field);

    let volume = entry.volume().ok().map(|v| match v {
        PermissiveType::Typed(n) => n.to_string(),
        PermissiveType::Chunks(chunks) => decode_field(&chunks),
    });

    let issue = entry.number().map(decode_field).ok();

    let doi = entry.doi().ok();

    let url = entry.get("url").map(|chunks| chunks.format_verbatim());

    // Decode the raw `pages` field rather than the normalized numeric range so the
    // source's dash convention is preserved: `--` renders as an en-dash and `---`
    // as an em-dash, while an existing single hyphen or literal dash is kept.
    let pages = entry
        .get("pages")
        .map(|chunks| normalize_page_dashes(&decode_field(chunks)));

    let abstract_text = entry.abstract_().map(decode_field).ok();

    let publisher = entry.publisher().ok().map(|chunks_vec| {
        chunks_vec
            .iter()
            .map(|chunks| decode_field(chunks))
            .collect::<Vec<_>>()
            .join("; ")
    });

    // Extract first PDF path from the `file` field (Zotero format: "path1;path2;...")
    let source_pdf = entry.get("file").and_then(|chunks| {
        let raw = chunks.format_verbatim();
        raw.split(';')
            .map(|s| s.trim())
            .find(|s| s.to_lowercase().ends_with(".pdf"))
            .map(|s| s.to_string())
    });

    ImportedPaper {
        source_pdf,
        paper: Paper {
            title,
            authors,
            year,
            doi,
            abstract_text,
            publication: Publication {
                journal,
                volume,
                issue,
                pages,
                publisher,
            },
            links: PaperLinks {
                url,
                ..Default::default()
            },
            ..Default::default()
        },
    }
}

/// Render a [`Person`] as a "First Last" display name, preserving name particles
/// (`von`) and suffixes (`III`).
///
/// The last name carries any leading particle (`von Hicks`); the first name
/// carries the given name and any suffix joined by a comma (`Michael, III`).
/// Reassembling the parts also reconstitutes a corporate name the backend split
/// on an internal preposition (`American Rights at Work`).
fn person_display(person: &Person) -> String {
    let mut last = String::new();
    if !person.prefix.is_empty() {
        last.push_str(&person.prefix);
    }
    if !person.name.is_empty() {
        if !last.is_empty() {
            last.push(' ');
        }
        last.push_str(&person.name);
    }

    let mut first = person.given_name.clone();
    if !person.suffix.is_empty() {
        if !first.is_empty() {
            first.push_str(", ");
        }
        first.push_str(&person.suffix);
    }

    match (first.is_empty(), last.is_empty()) {
        (true, _) => last,
        (false, true) => first,
        (false, false) => format!("{first} {last}"),
    }
}

/// Decode a field's chunk list into display text.
///
/// The backend leaves math (`$…$`) and rich-text commands (`\textit`, `\textbf`,
/// `\textsc`) unresolved; this reconstructs the LaTeX source, converts that markup
/// to the HTML-ish form Zotero emits, and strips residual TeX syntax.
fn decode_field(chunks: &[Spanned<Chunk>]) -> String {
    let latex = chunks_to_latex(chunks);
    let marked = map_tex_markup(&latex);
    strip_tex(&marked)
}

/// Reconstruct a LaTeX-ish string from a chunk list, re-wrapping math chunks in
/// `$…$` so the markup conversion can recognize sub/superscripts.
fn chunks_to_latex(chunks: &[Spanned<Chunk>]) -> String {
    let mut out = String::new();
    for c in chunks {
        match &c.v {
            Chunk::Normal(s) | Chunk::Verbatim(s) => out.push_str(s),
            Chunk::Math(s) => {
                out.push('$');
                out.push_str(s);
                out.push('$');
            }
        }
    }
    out
}

/// Superscript Unicode for a single math token, when one exists (digits and a few
/// symbols). Mirrors Zotero's reverse mapping table.
fn superscript_char(token: &str) -> Option<&'static str> {
    Some(match token {
        "0" => "\u{2070}",
        "1" => "\u{00B9}",
        "2" => "\u{00B2}",
        "3" => "\u{00B3}",
        "4" => "\u{2074}",
        "5" => "\u{2075}",
        "6" => "\u{2076}",
        "7" => "\u{2077}",
        "8" => "\u{2078}",
        "9" => "\u{2079}",
        "+" => "\u{207A}",
        "-" => "\u{207B}",
        "=" => "\u{207C}",
        "(" => "\u{207D}",
        ")" => "\u{207E}",
        "n" => "\u{207F}",
        _ => return None,
    })
}

/// Subscript Unicode for a single math token, when one exists.
fn subscript_char(token: &str) -> Option<&'static str> {
    Some(match token {
        "0" => "\u{2080}",
        "1" => "\u{2081}",
        "2" => "\u{2082}",
        "3" => "\u{2083}",
        "4" => "\u{2084}",
        "5" => "\u{2085}",
        "6" => "\u{2086}",
        "7" => "\u{2087}",
        "8" => "\u{2088}",
        "9" => "\u{2089}",
        "+" => "\u{208A}",
        "-" => "\u{208B}",
        "=" => "\u{208C}",
        "(" => "\u{208D}",
        ")" => "\u{208E}",
        _ => return None,
    })
}

/// The Greek-letter (and a couple of symbol) math commands Zotero resolves to a
/// Unicode character.
fn math_symbol(command: &str) -> Option<char> {
    Some(match command {
        "alpha" => 'α',
        "beta" => 'β',
        "gamma" => 'γ',
        "delta" => 'δ',
        "epsilon" => 'ε',
        "zeta" => 'ζ',
        "eta" => 'η',
        "theta" => 'θ',
        "iota" => 'ι',
        "kappa" => 'κ',
        "lambda" => 'λ',
        "mu" => 'μ',
        "nu" => 'ν',
        "xi" => 'ξ',
        "pi" => 'π',
        "rho" => 'ρ',
        "sigma" => 'σ',
        "tau" => 'τ',
        "upsilon" => 'υ',
        "phi" => 'φ',
        "chi" => 'χ',
        "psi" => 'ψ',
        "omega" => 'ω',
        "Gamma" => 'Γ',
        "Delta" => 'Δ',
        "Theta" => 'Θ',
        "Lambda" => 'Λ',
        "Xi" => 'Ξ',
        "Pi" => 'Π',
        "Sigma" => 'Σ',
        "Phi" => 'Φ',
        "Psi" => 'Ψ',
        "Omega" => 'Ω',
        "times" => '×',
        "sim" => '∼',
        _ => return None,
    })
}

/// Convert the rich-text LaTeX markup Zotero permits into its HTML-ish rendering:
/// `\textit`/`\textbf`/`\textsc` become tags, and `$…^{…}$` / `$…_{…}$` become
/// Unicode super/subscripts (or `<sup>`/`<sub>` when no Unicode form exists).
fn map_tex_markup(input: &str) -> String {
    let s = replace_braced_command(input, "textit", |inner| format!("<i>{inner}</i>"));
    let s = replace_braced_command(&s, "textbf", |inner| format!("<b>{inner}</b>"));
    let s = replace_braced_command(&s, "textsc", |inner| {
        format!("<span style=\"small-caps\">{inner}</span>")
    });
    convert_math(&s)
}

/// Replace every `\command{...}` occurrence (balanced braces) with `render(inner)`.
fn replace_braced_command(input: &str, command: &str, render: impl Fn(&str) -> String) -> String {
    let needle = format!("\\{command}{{");
    let mut out = String::new();
    let mut rest = input;

    while let Some(pos) = rest.find(&needle) {
        out.push_str(&rest[..pos]);
        let after = &rest[pos + needle.len()..];
        if let Some((inner, tail)) = split_balanced(after) {
            out.push_str(&render(inner));
            rest = tail;
        } else {
            // Unbalanced — emit the marker verbatim and move past it.
            out.push_str(&rest[pos..pos + needle.len()]);
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

/// Split `input` (which begins just inside an open brace) into the balanced-brace
/// content and the remainder after its matching close brace.
fn split_balanced(input: &str) -> Option<(&str, &str)> {
    let mut depth = 1usize;
    for (idx, ch) in input.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some((&input[..idx], &input[idx + 1..]));
                }
            }
            _ => {}
        }
    }
    None
}

/// Convert `$…$` math spans to display text: resolve Greek/symbol commands and
/// single super/subscripts, then drop the delimiters.
fn convert_math(input: &str) -> String {
    let mut out = String::new();
    let mut rest = input;

    while let Some(start) = rest.find('$') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        if let Some(end) = after.find('$') {
            let math = &after[..end];
            out.push_str(&render_math(math));
            rest = &after[end + 1..];
        } else {
            // No closing `$` — leave the rest as-is.
            out.push_str(&rest[start..]);
            return out;
        }
    }
    out.push_str(rest);
    out
}

/// Render the inside of a `$…$` math span.
///
/// Handles a leading symbol/Greek command (`\Sigma` -> Σ) followed by an optional
/// single super/subscript (`^{0}` -> ⁰, `_{2}` -> ₂). A super/subscript whose
/// content is not a single mappable token falls back to `<sup>`/`<sub>`, with any
/// `\textrm{…}` wrapper unwrapped.
fn render_math(math: &str) -> String {
    let mut out = String::new();
    let mut rest = math;

    // Leading text before any script: a symbol/Greek command (`\Sigma` -> Σ) or a
    // literal prefix (`4` in `4^{…}`).
    if let Some(after_bs) = rest.strip_prefix('\\') {
        let name_len = after_bs
            .find(|c: char| !c.is_ascii_alphabetic())
            .unwrap_or(after_bs.len());
        let (name, tail) = after_bs.split_at(name_len);
        if let Some(sym) = math_symbol(name) {
            out.push(sym);
            rest = tail;
        }
    } else {
        let prefix_len = rest.find(['^', '_']).unwrap_or(rest.len());
        out.push_str(&rest[..prefix_len]);
        rest = &rest[prefix_len..];
    }

    // Optional super/subscript: `^{...}` or `_{...}`.
    if let Some(script) = rest.strip_prefix('^') {
        out.push_str(&render_script(script, true));
    } else if let Some(script) = rest.strip_prefix('_') {
        out.push_str(&render_script(script, false));
    } else {
        out.push_str(rest);
    }

    out
}

/// Render a `^{…}`/`_{…}` script body (`body` begins at the `{`). A single
/// mappable token becomes its Unicode super/subscript; anything else becomes a
/// `<sup>`/`<sub>` tag with a `\textrm{…}` wrapper unwrapped.
fn render_script(body: &str, sup: bool) -> String {
    let Some(inner) = body.strip_prefix('{').and_then(|b| b.strip_suffix('}')) else {
        return body.to_string();
    };

    // Unwrap a `\textrm{…}` wrapper.
    let content = inner
        .strip_prefix("\\textrm{")
        .and_then(|c| c.strip_suffix('}'))
        .unwrap_or(inner);

    let mapped = if sup {
        superscript_char(content)
    } else {
        subscript_char(content)
    };

    match mapped {
        Some(u) => u.to_string(),
        None if sup => format!("<sup>{content}</sup>"),
        None => format!("<sub>{content}</sub>"),
    }
}

/// Convert a page range's ASCII dash runs to their typographic form: `---`
/// becomes an em-dash and `--` an en-dash. A single hyphen and any existing
/// Unicode dash are left untouched.
fn normalize_page_dashes(input: &str) -> String {
    input.replace("---", "\u{2014}").replace("--", "\u{2013}")
}

/// Strip residual TeX syntax from an already-markup-converted string: drop
/// grouping braces and unescape backslash-escaped specials.
fn strip_tex(input: &str) -> String {
    let mut out = String::new();
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                // A backslash before a special character unescapes to that
                // character; a doubled backslash collapses to one.
                match chars.peek() {
                    Some(&n) if "#$%&~_^{}\\".contains(n) => {
                        out.push(n);
                        chars.next();
                    }
                    _ => out.push('\\'),
                }
            }
            '{' | '}' => {
                // Grouping braces carry no display meaning.
            }
            _ => out.push(c),
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paper(input: &str) -> Paper {
        import_bibtex(input).unwrap().pop().unwrap().paper
    }

    #[test]
    fn page_range_double_dash_becomes_en_dash() {
        let p = paper("@article{k, title={T}, pages={493--518}}");
        assert_eq!(p.publication.pages.as_deref(), Some("493\u{2013}518"));
    }

    #[test]
    fn page_range_triple_dash_becomes_em_dash() {
        let p = paper("@article{k, title={T}, pages={493---518}}");
        assert_eq!(p.publication.pages.as_deref(), Some("493\u{2014}518"));
    }

    #[test]
    fn page_range_single_hyphen_preserved() {
        let p = paper("@article{k, title={T}, pages={71-80}}");
        assert_eq!(p.publication.pages.as_deref(), Some("71-80"));
    }

    #[test]
    fn fjournal_preferred_over_journal_abbreviation() {
        let p = paper(
            "@article{k, title={T}, journal={Adv. Math.}, fjournal={Advances in Mathematics}}",
        );
        assert_eq!(
            p.publication.journal.as_deref(),
            Some("Advances in Mathematics")
        );
    }

    #[test]
    fn italic_bold_smallcaps_become_tags() {
        let p = paper(
            r#"@article{k, title={A (\textit{Natrix cetti}) and \textbf{do not} their \textsc{species status}}}"#,
        );
        assert_eq!(
            p.title,
            "A (<i>Natrix cetti</i>) and <b>do not</b> their <span style=\"small-caps\">species status</span>"
        );
    }

    #[test]
    fn math_subscript_digit_becomes_unicode() {
        let p = paper(r#"@article{k, title={DNA$_{\textrm{2}}$ sequences}}"#);
        assert_eq!(p.title, "DNA\u{2082} sequences");
    }

    #[test]
    fn math_greek_with_superscript_digit() {
        let p = paper("@article{k, title=\"{Production of $\\Sigma^{0}$ Hyperon}\"}");
        assert_eq!(p.title, "Production of \u{03A3}\u{2070} Hyperon");
    }

    #[test]
    fn math_superscript_non_digit_uses_sup_tag() {
        let p = paper("@article{k, journal={Actes du $4^{\\textrm{ème}}$ Congrès}}");
        assert_eq!(
            p.publication.journal.as_deref(),
            Some("Actes du 4<sup>ème</sup> Congrès")
        );
    }

    #[test]
    fn author_particle_and_suffix_preserved() {
        let p = paper(r#"@book{k, author="von Hicks, III, Michael"}"#);
        assert_eq!(p.authors, vec!["Michael, III von Hicks".to_string()]);
    }

    #[test]
    fn corporate_author_with_internal_preposition() {
        let p = paper(r#"@misc{k, author={American Rights at Work}}"#);
        assert_eq!(p.authors, vec!["American Rights at Work".to_string()]);
    }

    #[test]
    fn lowercase_given_name_particle_kept() {
        let p = paper(r#"@techreport{k, author={sudhin jacob and Kishore Tiruveedhula}}"#);
        assert_eq!(
            p.authors,
            vec![
                "sudhin jacob".to_string(),
                "Kishore Tiruveedhula".to_string()
            ]
        );
    }

    #[test]
    fn tolerant_parse_recovers_entry_past_preamble() {
        let input = "@preamble{see https://x.edu/~kotz/p.html}\n\n\
             @Article{batsis,\n author={John A. Batsis and David Kotz},\n \
             title={A Rural Health},\n journal={BMC},\n year=2020,\n \
             URL={https://x.edu/~kotz/index.html},\n}";
        let papers = import_bibtex(input).unwrap();
        assert_eq!(papers.len(), 1);
        assert_eq!(papers[0].paper.title, "A Rural Health");
        assert_eq!(
            papers[0].paper.authors,
            vec!["John A. Batsis".to_string(), "David Kotz".to_string()]
        );
    }

    #[test]
    fn plain_title_and_pages_unchanged() {
        let p = paper("@article{k, title={Simple Title}, pages={12}}");
        assert_eq!(p.title, "Simple Title");
        assert_eq!(p.publication.pages.as_deref(), Some("12"));
    }
}
