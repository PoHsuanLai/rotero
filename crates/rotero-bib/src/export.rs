use std::fmt::Write;

use rotero_models::Paper;

pub fn export_bibtex(papers: &[Paper]) -> String {
    let mut output = String::new();

    for paper in papers {
        let key = match paper.citation.citation_key.as_deref() {
            Some(k) if !k.is_empty() => k.to_string(),
            _ => generate_cite_key(paper),
        };
        let _ = writeln!(output, "@article{{{key},");

        let mut fields: Vec<String> = Vec::new();

        fields.push(format!("  title = {{{}}}", sanitize_bibtex(&paper.title)));

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

        output.push_str(&fields.join(",\n"));
        output.push('\n');
        output.push_str("}\n\n");
    }

    output
}

fn sanitize_bibtex(s: &str) -> String {
    // Remove all braces — they're unreliable from metadata sources
    // and the outer `{...}` wrapper already protects the value
    s.replace(['{', '}'], "")
}

/// Format: `lastnameYeartitleword` (e.g., `eysenbach2019attention`).
pub fn generate_cite_key(paper: &Paper) -> String {
    let author_part = paper
        .authors
        .first()
        .map(|a| {
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

pub fn generate_unique_cite_key(paper: &Paper, existing_keys: &[String]) -> String {
    let base = generate_cite_key(paper);

    if !existing_keys.contains(&base) {
        return base;
    }

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
