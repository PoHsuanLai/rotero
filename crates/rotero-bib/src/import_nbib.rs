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

            // Full authors (FAU) carry given names in full; prefer them. `AU`
            // holds the abbreviated "Last Initials" form used as a fallback for
            // records lacking FAU (pre-2002 PubMed and some ERIC exports).
            let full_authors = collect_creators(&fields, "FAU", false);
            let backup_authors = collect_creators(&fields, "AU", true);
            let authors = if full_authors.is_empty() {
                backup_authors
            } else {
                full_authors
            };

            // DP field is like "2023 Jan 15" or "2023"; take the leading year.
            let year = get_field(&fields, "DP").and_then(|dp| parse_year(&dp));

            // DOI lives in an `AID` or `LID` line tagged `[doi]`; there may be
            // several `AID`/`LID` lines and only one bears the DOI.
            let doi = find_tagged(&fields, "AID", "[doi]")
                .or_else(|| find_tagged(&fields, "LID", "[doi]"));

            let abstract_text = get_field(&fields, "AB");
            let journal = get_field(&fields, "JT").or_else(|| get_field(&fields, "TA"));
            let volume = get_field(&fields, "VI");
            let issue = get_field(&fields, "IP");

            // Explicit page ranges (`PG`) win; an abbreviated end page is
            // expanded to its full form. Otherwise a `[pii]` article identifier
            // from `AID`/`LID` stands in as the page value.
            let pages = get_field(&fields, "PG")
                .map(|pg| expand_page_range(&pg))
                .or_else(|| find_tagged(&fields, "AID", "[pii]"))
                .or_else(|| find_tagged(&fields, "LID", "[pii]"));

            // `PB` is the publisher when present. For book-type records the
            // publisher is instead carried by the title field (`JT`).
            let publisher = get_field(&fields, "PB").or_else(|| {
                if is_book(&fields) {
                    get_field(&fields, "JT")
                } else {
                    None
                }
            });

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
                    publisher,
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

/// Return the value of the first line under `tag` that contains `marker`, with
/// the marker (and surrounding whitespace) stripped off.
fn find_tagged(fields: &[TagValue], tag: &str, marker: &str) -> Option<String> {
    fields
        .iter()
        .filter(|f| f.tag == tag)
        .find(|f| f.value.contains(marker))
        .map(|f| f.value.replace(marker, "").trim().to_string())
}

/// Extract the leading four-digit year from a `DP` value such as `2019`,
/// `2015 May`, `2022 Oct 7`, or `Apr 2019`.
fn parse_year(dp: &str) -> Option<i32> {
    dp.split(|c: char| !c.is_ascii_digit())
        .find(|tok| tok.len() == 4)
        .and_then(|tok| tok.parse::<i32>().ok())
}

/// Whether a record is a book, in which case its publisher is stored in the
/// title field rather than a `PB` line.
fn is_book(fields: &[TagValue]) -> bool {
    get_all(fields, "PT")
        .iter()
        .any(|pt| pt == "Book" || pt == "Books")
}

/// Collect author names for `tag` into "First Last" strings.
///
/// PubMed `FAU`/`AU` names come in two shapes. `FAU` is `Last, First` (comma
/// form). `AU` is `Last Initials` (space form) — but ERIC exports reuse the
/// `AU` tag for `Last, First` names, so a trailing run of capitals is first
/// rewritten into the comma form to normalize both into the same path.
///
/// `et al.` placeholders are dropped, and initials are expanded to spaced,
/// period-terminated form (`MJ` and `M J` both become `M. J.`).
fn collect_creators(fields: &[TagValue], tag: &str, is_backup: bool) -> Vec<String> {
    get_all(fields, tag)
        .into_iter()
        .filter(|a| a.trim() != "et al.")
        .map(|a| {
            let normalized = if is_backup {
                rewrite_trailing_initials(&a)
            } else {
                a
            };
            let (first, last) = split_name(&normalized);
            let first = expand_initials(&first);
            if first.is_empty() {
                last
            } else {
                format!("{first} {last}")
            }
        })
        .collect()
}

/// Rewrite a trailing run of capital letters into comma form, so `van Raaij MJ`
/// becomes `van Raaij, MJ`. Names already in `Last, First` form are untouched.
fn rewrite_trailing_initials(value: &str) -> String {
    if value.contains(',') {
        return value.to_string();
    }
    if let Some(idx) = value.rfind(' ') {
        let (head, tail) = value.split_at(idx);
        let initials = tail.trim();
        if !initials.is_empty() && initials.chars().all(|c| c.is_ascii_uppercase()) {
            return format!("{head}, {initials}");
        }
    }
    value.to_string()
}

/// Surname particles that stay attached to the last name in space-form names.
const PARTICLES: &[&str] = &[
    "van", "von", "der", "den", "de", "del", "della", "di", "da", "la", "le", "du", "des", "el",
    "al", "bin", "ibn", "ter", "ten", "af", "zu",
];

/// Split a name into (first, last).
///
/// A comma splits `Last, First` directly. Otherwise the final token is the
/// surname, extended leftward to include any surname particle that follows the
/// first given-name token (so `Ludwig van Beethoven` → first `Ludwig`, last
/// `van Beethoven`).
fn split_name(name: &str) -> (String, String) {
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    if let Some((last, first)) = name.split_once(',') {
        return (first.trim().to_string(), last.trim().to_string());
    }

    let tokens: Vec<&str> = name.split(' ').filter(|s| !s.is_empty()).collect();
    match tokens.len() {
        0 => (String::new(), String::new()),
        1 => (String::new(), tokens[0].to_string()),
        _ => {
            let last_start = (1..tokens.len() - 1)
                .find(|&i| PARTICLES.contains(&tokens[i].to_lowercase().as_str()))
                .unwrap_or(tokens.len() - 1);
            (
                tokens[..last_start].join(" "),
                tokens[last_start..].join(" "),
            )
        }
    }
}

/// Expand run-together initials in a given-name string to spaced, period-form.
///
/// A token that is a run of uppercase ASCII letters with no period (`MJ`, `LM`,
/// `I`) becomes `M. J.`, `L. M.`, `I.`. Tokens that are ordinary words or that
/// already carry a period (`Jimmy`, `F.`) are left unchanged.
fn expand_initials(first: &str) -> String {
    first
        .split_whitespace()
        .map(|tok| {
            if !tok.is_empty() && tok.chars().all(|c| c.is_ascii_uppercase()) {
                tok.chars()
                    .map(|c| format!("{c}."))
                    .collect::<Vec<_>>()
                    .join(" ")
            } else {
                tok.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Expand any abbreviated end page in a range to its full form.
///
/// PubMed shortens the end of a range that shares a prefix with the start, so
/// `6913-7` denotes `6913-6917` and `475-82` denotes `475-482`. Each `\d+-\d+`
/// span whose end is shorter than its start is expanded by borrowing the
/// missing high-order digits from the start. Ranges of equal length (`25-36`)
/// and non-numeric page values are left untouched.
fn expand_page_range(pages: &str) -> String {
    let bytes = pages.as_bytes();
    let mut out = String::with_capacity(pages.len());
    let mut i = 0;

    while i < bytes.len() {
        // Try to match a `\d+-\d+` span starting at i.
        let start_begin = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i > start_begin && i < bytes.len() && bytes[i] == b'-' {
            let start = &pages[start_begin..i];
            let dash = i;
            i += 1;
            let end_begin = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i > end_begin {
                let end = &pages[end_begin..i];
                out.push_str(start);
                out.push('-');
                if start.len() > end.len() {
                    out.push_str(&start[..start.len() - end.len()]);
                }
                out.push_str(end);
                continue;
            }
            // No digits after the dash: emit what we consumed verbatim.
            out.push_str(&pages[start_begin..dash + 1]);
            continue;
        }
        // Not a range start: emit consumed digits (if any) plus one more char.
        if i > start_begin {
            out.push_str(&pages[start_begin..i]);
        }
        if i < bytes.len() {
            out.push(bytes[i] as char);
            i += 1;
        }
    }

    out
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
        assert_eq!(papers[0].authors, vec!["A. Einstein"]);
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

    #[test]
    fn expands_runtogether_and_spaced_initials() {
        // FAU spaced initials and AU run-together initials both become "M. J.".
        let input = "PMID- 1\nTI  - T\nFAU - van Raaij, M J\nAU  - van Raaij MJ\n\n";
        assert_eq!(
            import_nbib(input).unwrap()[0].authors,
            vec!["M. J. van Raaij"]
        );

        // Single-letter initial gains a period.
        let input = "PMID- 1\nTI  - T\nFAU - Gout, I\nAU  - Gout I\n\n";
        assert_eq!(import_nbib(input).unwrap()[0].authors, vec!["I. Gout"]);

        // A given name followed by a bare initial keeps the name, adds a period.
        let input = "PMID- 1\nTI  - T\nFAU - Efird, Jimmy T\nAU  - Efird JT\n\n";
        assert_eq!(
            import_nbib(input).unwrap()[0].authors,
            vec!["Jimmy T. Efird"]
        );
    }

    #[test]
    fn au_comma_form_with_particles_and_multiword_surname() {
        // ERIC reuses AU for "Last, First"; particles fold into the surname and
        // no stray comma is left behind.
        let input = "PMID- 1\nTI  - T\nAU  - van Groen, Maaike M.\nAU  - Eggen, Theo J. H. M.\n\n";
        assert_eq!(
            import_nbib(input).unwrap()[0].authors,
            vec!["Maaike M. van Groen", "Theo J. H. M. Eggen"]
        );

        let input = "PMID- 1\nTI  - T\nAU  - San Pedro, Sweet\n\n";
        assert_eq!(
            import_nbib(input).unwrap()[0].authors,
            vec!["Sweet San Pedro"]
        );

        let input = "PMID- 1\nTI  - T\nAU  - Di Giacomo, F. Tony\n\n";
        assert_eq!(
            import_nbib(input).unwrap()[0].authors,
            vec!["F. Tony Di Giacomo"]
        );
    }

    #[test]
    fn drops_et_al_placeholder() {
        let input = "PMID- 1\nTI  - T\nAU  - Booker GW\nAU  - et al.\n\n";
        assert_eq!(import_nbib(input).unwrap()[0].authors, vec!["G. W. Booker"]);
    }

    #[test]
    fn expands_abbreviated_page_ranges() {
        let input = "PMID- 1\nTI  - T\nPG  - 6913-7\n\n";
        assert_eq!(
            import_nbib(input).unwrap()[0].publication.pages.as_deref(),
            Some("6913-6917")
        );

        let input = "PMID- 1\nTI  - T\nPG  - 475-82\n\n";
        assert_eq!(
            import_nbib(input).unwrap()[0].publication.pages.as_deref(),
            Some("475-482")
        );

        // Equal-length ranges are untouched.
        let input = "PMID- 1\nTI  - T\nPG  - 25-36\n\n";
        assert_eq!(
            import_nbib(input).unwrap()[0].publication.pages.as_deref(),
            Some("25-36")
        );
    }

    #[test]
    fn single_page_and_article_id_from_pii() {
        // A bare page id in LID [pii] fills in when there is no PG.
        let input = "PMID- 1\nTI  - T\nLID - 1042 [pii]\n\n";
        assert_eq!(
            import_nbib(input).unwrap()[0].publication.pages.as_deref(),
            Some("1042")
        );

        let input = "PMID- 1\nTI  - T\nLID - S1080-6032(22)00139-9 [pii]\n\n";
        assert_eq!(
            import_nbib(input).unwrap()[0].publication.pages.as_deref(),
            Some("S1080-6032(22)00139-9")
        );
    }

    #[test]
    fn doi_from_second_aid_line() {
        // The DOI is on the second AID line; the first is a PII.
        let input = "PMID- 1\nTI  - T\nAID - S1080-6032(22)00139-9 [pii]\nAID - 10.1016/j.wem.2022.07.008 [doi]\n\n";
        assert_eq!(
            import_nbib(input).unwrap()[0].doi.as_deref(),
            Some("10.1016/j.wem.2022.07.008")
        );
    }

    #[test]
    fn publisher_from_pb_and_from_book_title() {
        let input =
            "PMID- 1\nTI  - T\nPB  - National Center for Biotechnology Information (US)\n\n";
        assert_eq!(
            import_nbib(input).unwrap()[0]
                .publication
                .publisher
                .as_deref(),
            Some("National Center for Biotechnology Information (US)")
        );

        // Book-type records carry the publisher in the title field.
        let input = "PMID- 1\nTI  - T\nJT  - Brookes Publishing Company\nPT  - Books\n\n";
        assert_eq!(
            import_nbib(input).unwrap()[0]
                .publication
                .publisher
                .as_deref(),
            Some("Brookes Publishing Company")
        );
    }

    #[test]
    fn year_from_various_dp_forms() {
        for (dp, want) in [
            ("2019", 2019),
            ("2015 May", 2015),
            ("Apr 2019", 2019),
            ("Article 10 2019", 2019),
        ] {
            let input = format!("PMID- 1\nTI  - T\nDP  - {dp}\n\n");
            assert_eq!(import_nbib(&input).unwrap()[0].year, Some(want));
        }
    }
}
