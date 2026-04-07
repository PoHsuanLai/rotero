use biblib::{CitationParser, RisParser};
use rotero_models::{Paper, PaperLinks, Publication};

/// Parse an RIS format string into a list of Papers using biblib.
pub fn import_ris(input: &str) -> Result<Vec<Paper>, String> {
    let parser = RisParser::new();
    let citations = parser
        .parse(input)
        .map_err(|e| format!("Failed to parse RIS: {e}"))?;

    let papers: Vec<Paper> = citations
        .into_iter()
        .filter_map(|c| {
            if c.title.is_empty() {
                return None;
            }

            let authors = c
                .authors
                .into_iter()
                .map(|a| match a.given_name {
                    Some(given) => format!("{given} {}", a.name),
                    None => a.name,
                })
                .collect();

            let year = c.date.map(|d| d.year);

            let url = c.urls.into_iter().next();

            Some(Paper {
                title: c.title,
                authors,
                year,
                doi: c.doi,
                abstract_text: c.abstract_text,
                publication: Publication {
                    journal: c.journal,
                    volume: c.volume,
                    issue: c.issue,
                    pages: c.pages,
                    publisher: c.publisher,
                },
                links: PaperLinks {
                    url,
                    ..Default::default()
                },
                ..Default::default()
            })
        })
        .collect();

    if papers.is_empty() {
        return Err("No valid records found in RIS file".to_string());
    }

    Ok(papers)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_ris_import() {
        let input = r#"TY  - JOUR
TI  - A test paper
AU  - Smith, John
AU  - Doe, Jane
PY  - 2023///
DO  - 10.1234/test
JO  - Nature
VL  - 42
IS  - 3
SP  - 100
EP  - 110
AB  - This is the abstract.
PB  - Springer
UR  - https://example.com/paper
ER  -
"#;
        let papers = import_ris(input).unwrap();
        assert_eq!(papers.len(), 1);
        let p = &papers[0];
        assert_eq!(p.title, "A test paper");
        assert_eq!(p.year, Some(2023));
        assert_eq!(p.doi.as_deref(), Some("10.1234/test"));
        assert_eq!(p.publication.journal.as_deref(), Some("Nature"));
        assert_eq!(p.publication.volume.as_deref(), Some("42"));
        assert_eq!(p.publication.issue.as_deref(), Some("3"));
        assert_eq!(p.abstract_text.as_deref(), Some("This is the abstract."));
        assert_eq!(p.publication.publisher.as_deref(), Some("Springer"));
        assert_eq!(p.links.url.as_deref(), Some("https://example.com/paper"));
    }

    #[test]
    fn test_multiple_records() {
        let input = r#"TY  - JOUR
TI  - Paper One
AU  - A
ER  -
TY  - JOUR
TI  - Paper Two
AU  - B
ER  -
"#;
        let papers = import_ris(input).unwrap();
        assert_eq!(papers.len(), 2);
        assert_eq!(papers[0].title, "Paper One");
        assert_eq!(papers[1].title, "Paper Two");
    }
}
