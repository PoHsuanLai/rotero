use std::fmt::Write;

use rotero_models::Paper;

/// Export a list of Papers to BibTeX format.
/// Uses stored citation_key if available, otherwise generates one.
pub fn export_bibtex(papers: &[Paper]) -> String {
    let mut output = String::new();

    for paper in papers {
        let key = match paper.citation.citation_key.as_deref() {
            Some(k) if !k.is_empty() => k.to_string(),
            _ => generate_cite_key(paper),
        };
        let _ = writeln!(output, "@article{{{key},");

        // Collect fields, then write them — last field must not have trailing comma
        let mut fields: Vec<String> = Vec::new();

        fields.push(format!(
            "  title = {{{}}}",
            sanitize_bibtex(&paper.title)
        ));

        if !paper.authors.is_empty() {
            let authors_str = paper.authors.join(" and ");
            fields.push(format!("  author = {{{{{authors_str}}}}}"));
        }

        if let Some(year) = paper.year {
            fields.push(format!("  year = {{{year}}}"));
        }

        if let Some(ref journal) = paper.publication.journal {
            fields.push(format!("  journal = {{{{{journal}}}}}"));
        }

        if let Some(ref volume) = paper.publication.volume {
            fields.push(format!("  volume = {{{volume}}}"));
        }

        if let Some(ref issue) = paper.publication.issue {
            fields.push(format!("  number = {{{issue}}}"));
        }

        if let Some(ref pages) = paper.publication.pages {
            fields.push(format!("  pages = {{{pages}}}"));
        }

        if let Some(ref doi) = paper.doi {
            fields.push(format!("  doi = {{{doi}}}"));
        }

        if let Some(ref url) = paper.links.url {
            fields.push(format!("  url = {{{url}}}"));
        }

        if let Some(ref publisher) = paper.publication.publisher {
            fields.push(format!("  publisher = {{{{{publisher}}}}}"));
        }

        // Skip abstract — not needed for citation formatting and often contains
        // characters (unbalanced braces, HTML tags) that break BibTeX parsing

        // Join with commas — no trailing comma before closing brace
        output.push_str(&fields.join(",\n"));
        output.push('\n');
        output.push_str("}\n\n");
    }

    output
}


/// Sanitize a string for use inside BibTeX `{...}` delimiters.
/// Strips unbalanced braces that would break the parser.
fn sanitize_bibtex(s: &str) -> String {
    // Remove all braces — they're unreliable from metadata sources
    // and the outer `{...}` wrapper already protects the value
    s.replace('{', "").replace('}', "")
}

/// Generate a citation key from paper metadata.
/// Format: `lastnameYear` (e.g., `eysenbach2019`, `smith2023`).
/// Uses first author's last name + year. Falls back gracefully for missing data.
pub fn generate_cite_key(paper: &Paper) -> String {
    let author_part = paper
        .authors
        .first()
        .map(|a| {
            // Extract last name: take last word, lowercase, keep only ascii alphanumeric
            a.split_whitespace()
                .last()
                .unwrap_or("unknown")
                .to_lowercase()
                .chars()
                .filter(|c| c.is_ascii_alphanumeric())
                .collect::<String>()
        })
        .unwrap_or_else(|| "unknown".to_string());

    let year_part = paper
        .year
        .map(|y| y.to_string())
        .unwrap_or_else(|| "nd".to_string());

    // Add first significant title word for disambiguation
    let title_word = paper
        .title
        .split_whitespace()
        .find(|w| {
            let lower = w.to_lowercase();
            w.len() > 3
                && !matches!(
                    lower.as_str(),
                    "with"
                        | "from"
                        | "that"
                        | "this"
                        | "what"
                        | "when"
                        | "where"
                        | "which"
                        | "their"
                        | "there"
                        | "these"
                        | "those"
                        | "have"
                        | "been"
                        | "were"
                        | "will"
                        | "your"
                )
        })
        .map(|w| {
            w.to_lowercase()
                .chars()
                .filter(|c| c.is_ascii_alphanumeric())
                .collect::<String>()
        })
        .unwrap_or_else(|| "paper".to_string());

    format!("{author_part}{year_part}{title_word}")
}

/// Generate a unique citation key, appending a/b/c/... if the base key conflicts
/// with existing keys.
pub fn generate_unique_cite_key(paper: &Paper, existing_keys: &[String]) -> String {
    let base = generate_cite_key(paper);

    if !existing_keys.contains(&base) {
        return base;
    }

    // Append suffix: a, b, c, ...
    for suffix in b'a'..=b'z' {
        let candidate = format!("{base}{}", suffix as char);
        if !existing_keys.contains(&candidate) {
            return candidate;
        }
    }

    // Extremely unlikely: fall back to numeric suffix
    for i in 2..100 {
        let candidate = format!("{base}{i}");
        if !existing_keys.contains(&candidate) {
            return candidate;
        }
    }

    base
}
