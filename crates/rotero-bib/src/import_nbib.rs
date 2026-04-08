use rotero_models::{Paper, PaperLinks, Publication};

/// Parses an NBIB (PubMed/MEDLINE) string and returns the extracted papers.
///
/// NBIB is a tagged format exported by PubMed. Each field starts with a 4-character
/// tag (e.g. `PMID`, `TI  `, `AU  `) followed by `- ` and the value. Continuation
/// lines start with 6 spaces. Records are separated by blank lines.
pub fn import_nbib(input: &str) -> Result<Vec<Paper>, String> {
    let records = parse_records(input);

    let papers: Vec<Paper> = records
        .into_iter()
        .filter_map(|fields| {
            let title = get_field(&fields, "TI")?;

            let authors: Vec<String> = get_all(&fields, "AU")
                .into_iter()
                .map(|a| {
                    // NBIB authors are "Last FM" — convert to "First Last" if possible
                    if let Some((last, initials)) = a.split_once(' ') {
                        format!("{initials} {last}")
                    } else {
                        a
                    }
                })
                .collect();

            // Full authors (FAU) are "Last, First Middle" — prefer these if available
            let full_authors: Vec<String> = get_all(&fields, "FAU")
                .into_iter()
                .map(|a| {
                    if let Some((last, first)) = a.split_once(", ") {
                        format!("{first} {last}")
                    } else {
                        a
                    }
                })
                .collect();

            let authors = if full_authors.is_empty() {
                authors
            } else {
                full_authors
            };

            // DP field is like "2023 Jan 15" or "2023"
            let year = get_field(&fields, "DP").and_then(|dp| {
                dp.split_whitespace()
                    .next()
                    .and_then(|y| y.parse::<i32>().ok())
            });

            let doi = get_field(&fields, "AID").and_then(|aid| {
                // AID lines look like "10.1234/test [doi]"
                if aid.contains("[doi]") {
                    Some(aid.replace("[doi]", "").trim().to_string())
                } else {
                    None
                }
            });
            // Fallback: try LID field
            let doi = doi.or_else(|| {
                get_field(&fields, "LID").and_then(|lid| {
                    if lid.contains("[doi]") {
                        Some(lid.replace("[doi]", "").trim().to_string())
                    } else {
                        None
                    }
                })
            });

            let abstract_text = get_field(&fields, "AB");
            let journal = get_field(&fields, "JT")
                .or_else(|| get_field(&fields, "TA"));
            let volume = get_field(&fields, "VI");
            let issue = get_field(&fields, "IP");
            let pages = get_field(&fields, "PG");

            let pmid = get_field(&fields, "PMID");
            let url = pmid
                .as_ref()
                .map(|id| format!("https://pubmed.ncbi.nlm.nih.gov/{id}/"));

            Some(Paper {
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
                    publisher: None,
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
        return Err("No valid records found in NBIB file".to_string());
    }

    Ok(papers)
}

/// A parsed tag-value pair.
struct TagValue {
    tag: String,
    value: String,
}

/// Split input into records (separated by blank lines), then parse each record's tags.
fn parse_records(input: &str) -> Vec<Vec<TagValue>> {
    let mut records: Vec<Vec<TagValue>> = Vec::new();
    let mut current: Vec<TagValue> = Vec::new();

    for line in input.lines() {
        if line.trim().is_empty() {
            if !current.is_empty() {
                records.push(std::mem::take(&mut current));
            }
            continue;
        }

        // Continuation line: starts with spaces and no tag
        if line.starts_with("      ") {
            if let Some(last) = current.last_mut() {
                last.value.push(' ');
                last.value.push_str(line.trim());
            }
            continue;
        }

        // Tag line: "XXXX- value"
        if line.len() >= 6 && &line[4..6] == "- " {
            let tag = line[..4].trim().to_string();
            let value = line[6..].trim().to_string();
            current.push(TagValue { tag, value });
        }
    }

    if !current.is_empty() {
        records.push(current);
    }

    records
}

fn get_field(fields: &[TagValue], tag: &str) -> Option<String> {
    fields
        .iter()
        .find(|f| f.tag == tag)
        .map(|f| f.value.clone())
}

fn get_all(fields: &[TagValue], tag: &str) -> Vec<String> {
    fields
        .iter()
        .filter(|f| f.tag == tag)
        .map(|f| f.value.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_nbib_import() {
        let input = r#"PMID- 12345678
TI  - Attention is all you need
FAU - Vaswani, Ashish
FAU - Shazeer, Noam
AU  - Vaswani A
AU  - Shazeer N
DP  - 2017 Jun 12
AID - 10.5555/example [doi]
JT  - Advances in Neural Information Processing Systems
VI  - 30
IP  - 1
PG  - 5998-6008
AB  - The dominant sequence transduction models are based on complex recurrent or
      convolutional neural networks. We propose a new simple network architecture,
      the Transformer.

"#;
        let papers = import_nbib(input).unwrap();
        assert_eq!(papers.len(), 1);
        let p = &papers[0];
        assert_eq!(p.title, "Attention is all you need");
        assert_eq!(p.authors, vec!["Ashish Vaswani", "Noam Shazeer"]);
        assert_eq!(p.year, Some(2017));
        assert_eq!(p.doi.as_deref(), Some("10.5555/example"));
        assert_eq!(
            p.publication.journal.as_deref(),
            Some("Advances in Neural Information Processing Systems")
        );
        assert_eq!(p.publication.volume.as_deref(), Some("30"));
        assert_eq!(p.publication.issue.as_deref(), Some("1"));
        assert_eq!(p.publication.pages.as_deref(), Some("5998-6008"));
        assert!(p.abstract_text.as_ref().unwrap().contains("Transformer"));
        assert_eq!(
            p.links.url.as_deref(),
            Some("https://pubmed.ncbi.nlm.nih.gov/12345678/")
        );
    }

    #[test]
    fn test_multiple_records() {
        let input = r#"PMID- 111
TI  - Paper One
AU  - Smith J

PMID- 222
TI  - Paper Two
AU  - Doe A

"#;
        let papers = import_nbib(input).unwrap();
        assert_eq!(papers.len(), 2);
        assert_eq!(papers[0].title, "Paper One");
        assert_eq!(papers[1].title, "Paper Two");
    }

    #[test]
    fn test_fallback_to_au_when_no_fau() {
        let input = r#"PMID- 333
TI  - Short author format
AU  - Einstein A

"#;
        let papers = import_nbib(input).unwrap();
        assert_eq!(papers[0].authors, vec!["A Einstein"]);
    }

    #[test]
    fn test_doi_from_lid_fallback() {
        let input = r#"PMID- 444
TI  - LID DOI paper
AU  - Test A
LID - 10.1000/test.lid [doi]

"#;
        let papers = import_nbib(input).unwrap();
        assert_eq!(papers[0].doi.as_deref(), Some("10.1000/test.lid"));
    }

    #[test]
    fn test_empty_input() {
        let result = import_nbib("");
        assert!(result.is_err());
    }

    #[test]
    fn test_ta_fallback_for_journal() {
        let input = r#"PMID- 555
TI  - Abbreviated journal
AU  - Test A
TA  - Nature

"#;
        let papers = import_nbib(input).unwrap();
        assert_eq!(papers[0].publication.journal.as_deref(), Some("Nature"));
    }
}
