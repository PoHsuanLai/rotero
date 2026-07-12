use std::fmt::Write;

use rotero_models::{CreatorRole, Paper};

/// Maps a Zotero item type to the BibTeX entry type used on export. Types that
/// have no distinct BibTeX entry fall back to `misc`, and the journal-article
/// default keeps `article` so existing exports are unchanged.
fn bibtex_entry_type(item_type: &str) -> &'static str {
    match item_type {
        "journalArticle" | "magazineArticle" | "newspaperArticle" => "article",
        "book" => "book",
        "bookSection" | "encyclopediaArticle" | "dictionaryEntry" => "incollection",
        "conferencePaper" => "inproceedings",
        "thesis" => "phdthesis",
        "report" => "techreport",
        "manuscript" | "preprint" => "unpublished",
        "letter" | "email" | "instantMessage" => "misc",
        "webpage" | "blogPost" | "forumPost" => "misc",
        _ => "misc",
    }
}

/// Join a set of creators of a given role as a BibTeX name list, or `None` if
/// there are none. Names are wrapped in a protective brace group.
fn creator_field(paper: &Paper, role: &CreatorRole, bibtex_key: &str) -> Option<String> {
    let names: Vec<String> = paper
        .creators
        .iter()
        .filter(|c| &c.role == role)
        .map(|c| c.display_name())
        .filter(|s| !s.is_empty())
        .collect();
    if names.is_empty() {
        return None;
    }
    Some(format!("  {bibtex_key} = {{{{{}}}}}", names.join(" and ")))
}

/// Exports a slice of papers as a BibTeX string.
pub fn export_bibtex(papers: &[Paper]) -> String {
    let mut output = String::new();

    for paper in papers {
        let key = match paper.citation.citation_key.as_deref() {
            Some(k) if !k.is_empty() => k.to_string(),
            _ => generate_cite_key(paper),
        };
        let _ = writeln!(output, "@{}{{{key},", bibtex_entry_type(&paper.item_type));

        let mut fields: Vec<String> = Vec::new();

        fields.push(format!("  title = {{{}}}", sanitize_bibtex(&paper.title)));

        // Authors and editors export to their distinct BibTeX fields.
        if let Some(f) = creator_field(paper, &CreatorRole::Author, "author") {
            fields.push(f);
        }
        if let Some(f) = creator_field(paper, &CreatorRole::Editor, "editor") {
            fields.push(f);
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

        if let Some(ref series) = paper.publication.series {
            fields.push(format!("  series = {{{{{series}}}}}"));
        }

        if let Some(ref isbn) = paper.publication.isbn {
            fields.push(format!("  isbn = {{{isbn}}}"));
        }

        if let Some(ref issn) = paper.publication.issn {
            fields.push(format!("  issn = {{{issn}}}"));
        }

        if let Some(ref place) = paper.publication.place {
            fields.push(format!("  address = {{{{{place}}}}}"));
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
    // Prefer the first author's structured surname; fall back to the last token
    // of the display name for institutional/mononym creators.
    let author_part = paper
        .creators
        .iter()
        .find(|c| c.role.is_author())
        .map(|c| {
            let surname = if !c.last_name.is_empty() {
                c.last_name.clone()
            } else {
                c.display_name()
                    .split_whitespace()
                    .last()
                    .unwrap_or("unknown")
                    .to_string()
            };
            surname
                .to_lowercase()
                .chars()
                .filter(|c| c.is_ascii_alphanumeric())
                .collect::<String>()
        })
        .filter(|s| !s.is_empty())
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

/// Generates a citation key that does not collide with `existing_keys`,
/// appending a letter suffix (a-z) if the base key is taken.
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

#[cfg(test)]
mod tests {
    use super::*;
    use rotero_models::{Creator, CreatorRole, Paper};

    #[test]
    fn book_exports_as_book_entry() {
        let mut p = Paper::new("A Treatise".to_string());
        p.item_type = "book".to_string();
        p.year = Some(1999);
        p.creators = vec![Creator::author("Ada", "Lovelace")];
        p.publication.publisher = Some("Acme Press".into());
        p.publication.isbn = Some("978-0-13-468599-1".into());
        let bib = export_bibtex(&[p]);
        assert!(bib.contains("@book{"), "expected @book, got: {bib}");
        assert!(bib.contains("author = {{Ada Lovelace}}"));
        assert!(bib.contains("isbn = {978-0-13-468599-1}"));
    }

    #[test]
    fn journal_article_still_exports_as_article() {
        let mut p = Paper::new("A Paper".to_string());
        p.creators = vec![Creator::author("Jane", "Doe")];
        let bib = export_bibtex(&[p]);
        assert!(bib.contains("@article{"));
    }

    #[test]
    fn authors_and_editors_split_into_distinct_fields() {
        let mut p = Paper::new("Edited Volume".to_string());
        p.item_type = "bookSection".to_string();
        p.creators = vec![
            Creator::author("Ada", "Lovelace"),
            Creator {
                first_name: "Ed".into(),
                last_name: "Itor".into(),
                name: String::new(),
                role: CreatorRole::Editor,
            },
        ];
        let bib = export_bibtex(&[p]);
        assert!(bib.contains("author = {{Ada Lovelace}}"));
        assert!(bib.contains("editor = {{Ed Itor}}"));
    }

    #[test]
    fn cite_key_uses_structured_surname() {
        // "de Koning" would break the split-on-space surname guess; the
        // structured last_name gives the correct key.
        let mut p = Paper::new("Genome Assembly".to_string());
        p.year = Some(2020);
        p.creators = vec![Creator::author("A. P.", "de Koning")];
        let key = generate_cite_key(&p);
        assert!(key.starts_with("dekoning2020"), "got: {key}");
    }
}
