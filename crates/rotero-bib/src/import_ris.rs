//! RIS bibliography import.
//!
//! Parses the line-based RIS format into the flat [`Paper`] model. Each record
//! runs from a `TY` (type) tag to its `ER` (end of reference) tag. Tag lines have
//! the shape `XX  - value`; a line that does not open with a valid tag is a
//! continuation of the previous tag's value (RIS wraps long fields — titles,
//! abstracts — across physical lines).
//!
//! Field selection follows Zotero's `RIS` translator over the subset the flat
//! model represents. The mapping of a few tags (the publication title, the issue)
//! depends on the record's item type, so the type is resolved first.

use rotero_models::{Paper, PaperLinks, Publication};

/// Parses an RIS string and returns the extracted papers, one per `TY`…`ER`
/// record. Records without a title are kept: RIS exports carry title-less
/// records (statistics rows, placeholders) that the caller may still want.
pub fn import_ris(input: &str) -> Result<Vec<Paper>, String> {
    let records = split_records(input);
    if records.is_empty() {
        return Err("No valid records found in RIS file".to_string());
    }
    Ok(records.iter().map(record_to_paper).collect())
}

/// A single tag occurrence within a record: the two-letter tag and its value.
struct Field {
    tag: String,
    value: String,
}

/// One RIS record: an ordered list of its tag/value fields.
struct Record {
    fields: Vec<Field>,
}

impl Record {
    /// The value of the first occurrence of `tag`, if present and non-empty.
    fn first(&self, tag: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|f| f.tag == tag && !f.value.is_empty())
            .map(|f| f.value.as_str())
    }
}

/// Split the input into records. Each record starts at a `TY` tag and ends at the
/// matching `ER` tag (or the next `TY`). Continuation lines — physical lines that
/// do not begin with a valid `XX  - ` tag — are appended to the value of the tag
/// that precedes them, joined with a space so wrapped titles read as one line.
fn split_records(input: &str) -> Vec<Record> {
    let mut records = Vec::new();
    let mut current: Option<Record> = None;

    for raw in input.lines() {
        let line = raw.trim_end_matches(['\r', '\n']);
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        match parse_tag_line(line) {
            Some((tag, value)) => {
                if tag == "TY" {
                    if let Some(rec) = current.take() {
                        records.push(rec);
                    }
                    current = Some(Record { fields: Vec::new() });
                }
                if tag == "ER" {
                    if let Some(rec) = current.take() {
                        records.push(rec);
                    }
                    continue;
                }
                if let Some(rec) = current.as_mut() {
                    rec.fields.push(Field {
                        tag: tag.to_string(),
                        value: value.to_string(),
                    });
                }
            }
            None => {
                // Continuation of the previous field's value.
                if let Some(field) = current.as_mut().and_then(|r| r.fields.last_mut()) {
                    if !field.value.is_empty() {
                        field.value.push(' ');
                    }
                    field.value.push_str(trimmed);
                }
            }
        }
    }

    if let Some(rec) = current.take() {
        records.push(rec);
    }
    records
}

/// Parse a physical line into a `(tag, value)` pair when it opens with a valid RIS
/// tag. Accepts the canonical `XX  - value`, the trailing-empty `XX  -`, and the
/// single-space `XX - value` variant some exporters emit; returns `None` for any
/// line that is not a tag line (i.e. a continuation line).
fn parse_tag_line(line: &str) -> Option<(&str, &str)> {
    // A tag is exactly two alphanumeric characters, followed by a separator that
    // is some spaces, a hyphen, then optional space(s).
    if line.len() < 2 {
        return None;
    }
    let tag = &line[..2];
    if !tag.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return None;
    }

    let rest = &line[2..];
    let after_spaces = rest.trim_start_matches(' ');
    let consumed_space = rest.len() != after_spaces.len();
    let after_hyphen = after_spaces.strip_prefix('-')?;
    // Require a genuine separator so a continuation line like `12-34 metres`
    // (two alphanumerics then a hyphen) is not misread as a tag: a space must
    // precede the hyphen, or the value must be empty (`ER  -`) or open with a
    // space (`XX - value`).
    if !consumed_space && !after_hyphen.is_empty() && !after_hyphen.starts_with(' ') {
        return None;
    }
    Some((tag, after_hyphen.trim()))
}

/// Resolve a record's Zotero item type from its `TY` tag, returned as a small
/// classification the field mapping needs.
fn item_type(record: &Record) -> ItemType {
    let ty = record.first("TY").unwrap_or("").to_ascii_uppercase();
    match ty.as_str() {
        // Types whose `AU` creator is not an author role.
        "ART" | "CHART" | "FIGURE" => ItemType::NonAuthorCreator,
        "COMP" => ItemType::NonAuthorCreator,
        "MPCT" | "ADVS" | "MULTI" | "VIDEO" => ItemType::NonAuthorCreator,
        "MAP" => ItemType::NonAuthorCreator,
        "MUSIC" | "SOUND" => ItemType::NonAuthorCreator,
        "PAT" => ItemType::NonAuthorCreator,
        "SLIDE" => ItemType::NonAuthorCreator,
        // The journal-article family: `T2` is the publication title and `M1`
        // carries the issue (an EndNote convention).
        "JOUR" | "EJOUR" | "JFULL" | "ABST" | "MGZN" | "NEWS" | "CTLG" => ItemType::JournalFamily,
        // Unknown / generic types default to journalArticle, so they inherit the
        // journal-family field mapping.
        _ if !KNOWN_TYPES.contains(&ty.as_str()) => ItemType::JournalFamily,
        _ => ItemType::Other,
    }
}

/// The record's item-type classification, as far as field mapping cares.
#[derive(PartialEq)]
enum ItemType {
    /// journalArticle / magazineArticle / newspaperArticle and their aliases.
    JournalFamily,
    /// A type whose `AU` maps to a non-author creator (artist, inventor, …).
    NonAuthorCreator,
    /// Any other known type (book, thesis, report, …).
    Other,
}

/// RIS `TY` values that map to a concrete non-journal item type. A `TY` outside
/// this set is unknown and defaults to journalArticle in Zotero.
const KNOWN_TYPES: &[&str] = &[
    "BOOK", "CHAP", "CASE", "CONF", "CPAPER", "DICT", "ENCYC", "ICOMM", "DATA", "DBASE", "HEAR",
    "PCOMM", "MANSCPT", "PAT", "RPRT", "STAT", "THES", "ELEC", "BILL", "LEGAL", "GOVDOC", "STAND",
    "MAP", "MUSIC", "SOUND", "COMP", "SLIDE", "ART", "CHART", "FIGURE", "MPCT", "ADVS", "MULTI",
    "VIDEO", "EBOOK", "ECHAP", "EDBOOK", "SER", "PAMP", "CLSWK", "INPR", "UNPD",
];

/// Build a [`Paper`] from one record, selecting the modeled fields per the item
/// type.
fn record_to_paper(record: &Record) -> Paper {
    let kind = item_type(record);

    let title = record
        .first("TI")
        .or_else(|| record.first("T1"))
        .unwrap_or("")
        .to_string();

    let authors = authors(record, &kind);

    let year = record_year(record);

    let doi = record.first("DO").map(str::to_string);

    // Publication title: `T2` for the journal family, then the always-mapped `JF`,
    // finally the journal abbreviation (`J2` / `JO`) promoted when nothing else
    // supplied a title.
    let journal = record
        .first("JF")
        .filter(|_| kind == ItemType::JournalFamily)
        .or_else(|| {
            record
                .first("T2")
                .filter(|_| kind == ItemType::JournalFamily)
        })
        .or_else(|| record.first("JF"))
        .map(str::to_string)
        .or_else(|| {
            record
                .first("J2")
                .or_else(|| record.first("JO"))
                .or_else(|| record.first("JA"))
                .map(str::to_string)
        });

    // Issue: `IS` normally, but the journal family also honors `M1` (EndNote
    // exports the issue there), preferring an explicit `IS`.
    let issue = record
        .first("IS")
        .or_else(|| {
            record
                .first("M1")
                .filter(|_| kind == ItemType::JournalFamily)
        })
        .map(str::to_string);

    let volume = record.first("VL").map(str::to_string);
    let pages = pages(record);
    let publisher = record.first("PB").map(str::to_string);
    let abstract_text = record
        .first("AB")
        .or_else(|| record.first("N2"))
        .map(str::to_string);
    let url = record.first("UR").map(str::to_string);

    Paper {
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
    }
}

/// Collect the author display names from `AU` and `A1` tags. Other creator tags
/// (`A2`/`A3`/`A4`/`ED`/`TA`) carry non-author roles (editors, translators,
/// series editors) and are excluded, as is every creator when the item type maps
/// `AU` to a non-author role.
fn authors(record: &Record, kind: &ItemType) -> Vec<String> {
    if *kind == ItemType::NonAuthorCreator {
        return Vec::new();
    }
    // `AU` and `A1` are the same author role; keep them in document order so a
    // leading `A1` is not shuffled after the `AU` block.
    record
        .fields
        .iter()
        .filter(|f| (f.tag == "AU" || f.tag == "A1") && !f.value.is_empty())
        .map(|f| format_author(&f.value))
        .collect()
}

/// Render an RIS author into a "First Last" display name. RIS authors are written
/// `Last, First`; the given name (after the first comma) has bare initials
/// expanded to spaced, period form, and surname particles stay in place. A name
/// with no comma is an institutional or single-field name and is passed through
/// unchanged.
fn format_author(raw: &str) -> String {
    let raw = raw.trim();
    match raw.split_once(',') {
        Some((last, given)) => {
            let last = last.trim();
            let given = expand_initials(given.trim());
            if given.is_empty() {
                last.to_string()
            } else {
                format!("{given} {last}")
            }
        }
        None => raw.to_string(),
    }
}

/// Expand bare initials in a given-name string to spaced, period form. A token of
/// one or two uppercase ASCII letters with no period (`J`, `AB`) becomes `J.`,
/// `A. B.`; RIS often writes initials this way. Longer all-caps tokens are treated
/// as names (`JASON` stays `JASON`), and ordinary words (`Jane`, `van`) or tokens
/// that already carry a period (`J.`, `A.S.`) are left unchanged. The two-letter
/// cap keeps real names from being shattered into false initials.
fn expand_initials(given: &str) -> String {
    given
        .split_whitespace()
        .map(|tok| {
            let is_bare_initials =
                matches!(tok.len(), 1 | 2) && tok.chars().all(|c| c.is_ascii_uppercase());
            if is_bare_initials {
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

/// Extract the publication year from the record's date tags. `DA` and `PY` carry
/// the date; `Y1` is the deprecated spelling and is consulted only when neither
/// `DA` nor `PY` is present.
fn record_year(record: &Record) -> Option<i32> {
    let date = record
        .first("DA")
        .or_else(|| record.first("PY"))
        .or_else(|| record.first("Y1"))?;
    parse_year(date)
}

/// Parse a four-digit year out of an RIS date string (`YYYY`, `YYYY/MM/DD`,
/// `YYYY-MM-DD`, …). A leading `0000` placeholder yields `None`.
fn parse_year(date: &str) -> Option<i32> {
    let head = date.split(['/', '-', ' ']).next().unwrap_or(date).trim();
    let year: i32 = head.parse().ok()?;
    if year == 0 { None } else { Some(year) }
}

/// Build the page range from `SP` (start) and `EP` (end). Zotero joins the two
/// with a hyphen and never abbreviates the end page, so `SP 6913` / `EP 6917`
/// yields `6913-6917`. `SP` alone (already a range, or a single page) is returned
/// as-is.
fn pages(record: &Record) -> Option<String> {
    let sp = record.first("SP").map(str::to_string);
    let ep = record.first("EP");
    match (sp, ep) {
        (Some(sp), Some(ep)) if !sp.contains('-') => Some(format!("{sp}-{ep}")),
        (Some(sp), _) => Some(sp),
        (None, Some(ep)) => Some(ep.to_string()),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_ris_import() {
        let input = "TY  - JOUR\nTI  - A test paper\nAU  - Smith, John\nAU  - Doe, Jane\n\
PY  - 2023///\nDO  - 10.1234/test\nJO  - Nature\nVL  - 42\nIS  - 3\nSP  - 100\nEP  - 110\n\
AB  - This is the abstract.\nPB  - Springer\nUR  - https://example.com/paper\nER  -\n";
        let papers = import_ris(input).unwrap();
        assert_eq!(papers.len(), 1);
        let p = &papers[0];
        assert_eq!(p.title, "A test paper");
        assert_eq!(p.authors, vec!["John Smith", "Jane Doe"]);
        assert_eq!(p.year, Some(2023));
        assert_eq!(p.doi.as_deref(), Some("10.1234/test"));
        assert_eq!(p.publication.journal.as_deref(), Some("Nature"));
        assert_eq!(p.publication.volume.as_deref(), Some("42"));
        assert_eq!(p.publication.issue.as_deref(), Some("3"));
        assert_eq!(p.publication.pages.as_deref(), Some("100-110"));
        assert_eq!(p.abstract_text.as_deref(), Some("This is the abstract."));
        assert_eq!(p.publication.publisher.as_deref(), Some("Springer"));
        assert_eq!(p.links.url.as_deref(), Some("https://example.com/paper"));
    }

    #[test]
    fn test_multiple_records() {
        let input = "TY  - JOUR\nTI  - Paper One\nAU  - A\nER  -\n\
TY  - JOUR\nTI  - Paper Two\nAU  - B\nER  -\n";
        let papers = import_ris(input).unwrap();
        assert_eq!(papers.len(), 2);
        assert_eq!(papers[0].title, "Paper One");
        assert_eq!(papers[1].title, "Paper Two");
    }

    /// A title wrapped across two physical lines is joined into one value.
    #[test]
    fn test_multiline_title_continuation() {
        let input = "TY  - JOUR\nT1  - Blood-brain barrier breach following\n\
cortical contusion in the rat\nJO  - J.Neurosurg.\nER  -\n";
        let papers = import_ris(input).unwrap();
        assert_eq!(
            papers[0].title,
            "Blood-brain barrier breach following cortical contusion in the rat"
        );
    }

    /// The single-space `XX - value` separator variant is accepted and does not
    /// leak a `- ` prefix into the value.
    #[test]
    fn test_single_space_separator() {
        let input = "TY - JOUR\nTI - Rapid identification of loci\nT2 - Molecular Ecology Resources\n\
VL - 9999\nM1 - 9999\nPY - 2009\nER -\n";
        let p = &import_ris(input).unwrap()[0];
        assert_eq!(p.title, "Rapid identification of loci");
        assert_eq!(
            p.publication.journal.as_deref(),
            Some("Molecular Ecology Resources")
        );
        assert_eq!(p.publication.volume.as_deref(), Some("9999"));
        assert_eq!(p.publication.issue.as_deref(), Some("9999"));
        assert_eq!(p.year, Some(2009));
    }

    /// Author initials keep their periods and surname particles stay attached to
    /// the given name (`Last, A. P. JASON de` → `A. P. JASON de Last`).
    #[test]
    fn test_author_initials_and_particles() {
        let input = "TY - JOUR\nTI - X\nAU - KONING, A. P. JASON de\nAU - Baldwin, S.A.\nER -\n";
        let p = &import_ris(input).unwrap()[0];
        assert_eq!(p.authors, vec!["A. P. JASON de KONING", "S.A. Baldwin"]);
    }

    /// Bare initials (no period) are expanded to spaced period form, while real
    /// given names and already-punctuated initials pass through unchanged.
    #[test]
    fn test_bare_initials_expanded() {
        let input = "TY  - JOUR\nTI  - X\nAU  - Jiang, J\nAU  - Smith, AB\n\
AU  - Doe, Jane\nAU  - Chen, A.S.\nER  -\n";
        let p = &import_ris(input).unwrap()[0];
        assert_eq!(
            p.authors,
            vec!["J. Jiang", "A. B. Smith", "Jane Doe", "A.S. Chen"]
        );
    }

    /// Non-author creator tags (`A2`/`A3`/`A4`/`TA`) never reach the author list.
    #[test]
    fn test_non_author_creators_excluded() {
        let input = "TY  - JOUR\nTI  - X\nA2  - Editor, Series\nA4  - Translator\n\
AU  - Name1, Author\nAU  - Name2, Author\nTA  - Author, Translated\nER  -\n";
        let p = &import_ris(input).unwrap()[0];
        assert_eq!(p.authors, vec!["Author Name1", "Author Name2"]);
    }

    /// `A1` and `AU` are the same role and stay in document order: a leading
    /// `A1` is not shuffled behind the `AU` block.
    #[test]
    fn test_a1_and_au_author_order() {
        let input = "TY  - JOUR\nTI  - X\nA1  - Georgiev, Danko\nAU  - Bello, Leon\n\
AU  - Carmi, Avishy\nER  -\n";
        let p = &import_ris(input).unwrap()[0];
        assert_eq!(
            p.authors,
            vec!["Danko Georgiev", "Leon Bello", "Avishy Carmi"]
        );
    }

    /// A type whose `AU` maps to a non-author role (artwork) yields no authors.
    #[test]
    fn test_non_author_item_type() {
        let input = "TY  - ART\nTI  - X\nAU  - By, Created\nER  -\n";
        let p = &import_ris(input).unwrap()[0];
        assert!(p.authors.is_empty());
    }

    /// `DA` wins over the deprecated `Y1`, even when `Y1` appears first.
    #[test]
    fn test_da_overrides_deprecated_y1() {
        let input = "TY  - JOUR\nTI  - X\nY1  - 1900/01/01\nDA  - 1950/01/01\nER  -\n";
        let p = &import_ris(input).unwrap()[0];
        assert_eq!(p.year, Some(1950));
    }

    /// The end page is not abbreviated: `SP 6913` / `EP 6917` → `6913-6917`.
    #[test]
    fn test_full_page_range() {
        let input = "TY  - JOUR\nTI  - X\nSP  - 6913\nEP  - 6917\nER  -\n";
        let p = &import_ris(input).unwrap()[0];
        assert_eq!(p.publication.pages.as_deref(), Some("6913-6917"));
    }

    /// DOI case is preserved (`10.17910/B7.1322`, not lowercased).
    #[test]
    fn test_doi_case_preserved() {
        let input = "TY  - DATA\nT1  - X\nDO  - 10.17910/B7.1322\nER  -\n";
        let p = &import_ris(input).unwrap()[0];
        assert_eq!(p.doi.as_deref(), Some("10.17910/B7.1322"));
    }

    /// A title-less record is retained, and its `T2` becomes the publication
    /// title for the journal family.
    #[test]
    fn test_titleless_record_kept() {
        let input =
            "TY  - JOUR\r\nT2  - Prostate Cancer Statistics 2015\r\nPY  - 0000\r\nER  - \r\n";
        let papers = import_ris(input).unwrap();
        assert_eq!(papers.len(), 1);
        assert_eq!(papers[0].title, "");
        assert_eq!(
            papers[0].publication.journal.as_deref(),
            Some("Prostate Cancer Statistics 2015")
        );
        assert_eq!(papers[0].year, None);
    }

    /// CRLF line endings parse the same as LF.
    #[test]
    fn test_crlf_line_endings() {
        let input = "TY  - JOUR\r\nTI  - Focus groups\r\nVL  - 100\r\nIS  - 6\r\n\
SP  - 674\r\nEP  - 682\r\nAU  - Bryan, C.J.\r\nER  - \r\n";
        let p = &import_ris(input).unwrap()[0];
        assert_eq!(p.title, "Focus groups");
        assert_eq!(p.publication.pages.as_deref(), Some("674-682"));
        assert_eq!(p.authors, vec!["C.J. Bryan"]);
    }
}
