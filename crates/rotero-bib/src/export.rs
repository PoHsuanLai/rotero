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

        let _ = writeln!(output, "  title = {{{{{}}}}},", paper.title);

        if !paper.authors.is_empty() {
            let authors_str = paper.authors.join(" and ");
            let _ = writeln!(output, "  author = {{{{{authors_str}}}}},");
        }

        if let Some(year) = paper.year {
            let _ = writeln!(output, "  year = {{{year}}},");
        }

        if let Some(ref journal) = paper.publication.journal {
            let _ = writeln!(output, "  journal = {{{{{journal}}}}},");
        }

        if let Some(ref volume) = paper.publication.volume {
            let _ = writeln!(output, "  volume = {{{volume}}},");
        }

        if let Some(ref issue) = paper.publication.issue {
            let _ = writeln!(output, "  number = {{{issue}}},");
        }

        if let Some(ref pages) = paper.publication.pages {
            let _ = writeln!(output, "  pages = {{{pages}}},");
        }

        if let Some(ref doi) = paper.doi {
            let _ = writeln!(output, "  doi = {{{doi}}},");
        }

        if let Some(ref url) = paper.links.url {
            let _ = writeln!(output, "  url = {{{url}}},");
        }

        if let Some(ref publisher) = paper.publication.publisher {
            let _ = writeln!(output, "  publisher = {{{{{publisher}}}}},");
        }

        if let Some(ref abstract_text) = paper.abstract_text {
            let _ = writeln!(output, "  abstract = {{{{{abstract_text}}}}},");
        }

        output.push_str("}\n\n");
    }

    output
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
