use hayagriva::archive::{self, ArchivedStyle};
use hayagriva::citationberg;
use hayagriva::{
    BibliographyDriver, BibliographyRequest, BufWriteFormat, CitationItem, CitationRequest,
};
use rotero_models::Paper;

use crate::export::export_bibtex;

/// Supported citation styles as `(display_name, style)` pairs.
pub const AVAILABLE_STYLES: &[(&str, ArchivedStyle)] = &[
    ("APA 7th", ArchivedStyle::AmericanPsychologicalAssociation),
    ("Chicago Author-Date", ArchivedStyle::ChicagoAuthorDate),
    ("Chicago Notes", ArchivedStyle::ChicagoNotes),
    (
        "Harvard Cite Them Right",
        ArchivedStyle::HarvardCiteThemRight,
    ),
    ("Vancouver", ArchivedStyle::Vancouver),
    ("MLA 9th", ArchivedStyle::ModernLanguageAssociation),
    ("Nature", ArchivedStyle::Nature),
    ("ACM", ArchivedStyle::AssociationForComputingMachinery),
    ("ACS", ArchivedStyle::AmericanChemicalSociety),
    ("AMA", ArchivedStyle::AmericanMedicalAssociation),
    ("AIP", ArchivedStyle::AmericanInstituteOfPhysics),
    ("APS", ArchivedStyle::AmericanPhysicsSociety),
    (
        "Springer Basic Author-Date",
        ArchivedStyle::SpringerBasicAuthorDate,
    ),
    ("Elsevier Harvard", ArchivedStyle::ElsevierHarvard),
];

/// Converts Papers → BibTeX string → hayagriva Library → formatted output.
pub fn format_bibliography(papers: &[Paper], style: ArchivedStyle) -> Result<String, String> {
    let csl_style = match style.get() {
        citationberg::Style::Independent(s) => s,
        citationberg::Style::Dependent(_) => return Err("Dependent styles not supported".into()),
    };

    let locales = archive::locales();

    let bibtex = export_bibtex(papers);
    let library = hayagriva::io::from_biblatex_str(&bibtex)
        .map_err(|errs| format!("BibTeX conversion errors: {:?}", errs))?;

    let entries: Vec<hayagriva::Entry> = library.iter().cloned().collect();
    if entries.is_empty() {
        return Ok(String::new());
    }

    let mut driver = BibliographyDriver::new();

    for entry in &entries {
        let items = vec![CitationItem::with_entry(entry)];
        driver.citation(CitationRequest::from_items(items, &csl_style, &locales));
    }

    let result = driver.finish(BibliographyRequest {
        style: &csl_style,
        locale: None,
        locale_files: &locales,
    });

    let mut output = String::new();

    if let Some(bib) = result.bibliography {
        for item in bib.items {
            let mut buf = String::new();
            item.content
                .write_buf(&mut buf, BufWriteFormat::Plain)
                .map_err(|e| e.to_string())?;
            output.push_str(&buf);
            output.push('\n');
        }
    }

    Ok(output)
}

/// Formats a single paper's citation in the given CSL style.
pub fn format_citation(paper: &Paper, style: ArchivedStyle) -> Result<String, String> {
    format_bibliography(std::slice::from_ref(paper), style)
}

/// Formats inline citations for papers (e.g. "(Smith, 2024)").
///
/// Returns one inline citation string per paper, plus a combined citation
/// for all papers grouped together (e.g. "(Smith, 2024; Jones, 2023)").
pub fn format_inline_citations(
    papers: &[Paper],
    style: ArchivedStyle,
) -> Result<(Vec<String>, String), String> {
    let csl_style = match style.get() {
        citationberg::Style::Independent(s) => s,
        citationberg::Style::Dependent(_) => return Err("Dependent styles not supported".into()),
    };

    let locales = archive::locales();
    let bibtex = export_bibtex(papers);
    let library = hayagriva::io::from_biblatex_str(&bibtex)
        .map_err(|errs| format!("BibTeX conversion errors: {:?}", errs))?;

    let entries: Vec<hayagriva::Entry> = library.iter().cloned().collect();
    if entries.is_empty() {
        return Ok((Vec::new(), String::new()));
    }

    // Individual citations (one per paper)
    let mut individual = Vec::new();
    for entry in &entries {
        let mut driver = BibliographyDriver::new();
        let items = vec![CitationItem::with_entry(entry)];
        driver.citation(CitationRequest::from_items(items, &csl_style, &locales));
        let result = driver.finish(BibliographyRequest {
            style: &csl_style,
            locale: None,
            locale_files: &locales,
        });
        let text = result
            .citations
            .first()
            .map(|c| {
                let mut buf = String::new();
                let _ = c.citation.write_buf(&mut buf, BufWriteFormat::Plain);
                buf
            })
            .unwrap_or_default();
        individual.push(text);
    }

    // Combined citation (all papers in one parenthetical)
    let mut driver = BibliographyDriver::new();
    let items: Vec<_> = entries.iter().map(CitationItem::with_entry).collect();
    driver.citation(CitationRequest::from_items(items, &csl_style, &locales));
    let result = driver.finish(BibliographyRequest {
        style: &csl_style,
        locale: None,
        locale_files: &locales,
    });
    let combined = result
        .citations
        .first()
        .map(|c| {
            let mut buf = String::new();
            let _ = c.citation.write_buf(&mut buf, BufWriteFormat::Plain);
            buf
        })
        .unwrap_or_default();

    Ok((individual, combined))
}

/// Formats bibliography entries, returning one string per paper.
pub fn format_bibliography_entries(
    papers: &[Paper],
    style: ArchivedStyle,
) -> Result<Vec<String>, String> {
    let csl_style = match style.get() {
        citationberg::Style::Independent(s) => s,
        citationberg::Style::Dependent(_) => return Err("Dependent styles not supported".into()),
    };

    let locales = archive::locales();
    let bibtex = export_bibtex(papers);
    let library = hayagriva::io::from_biblatex_str(&bibtex)
        .map_err(|errs| format!("BibTeX conversion errors: {:?}", errs))?;

    let entries: Vec<hayagriva::Entry> = library.iter().cloned().collect();
    if entries.is_empty() {
        return Ok(Vec::new());
    }

    let mut driver = BibliographyDriver::new();
    for entry in &entries {
        let items = vec![CitationItem::with_entry(entry)];
        driver.citation(CitationRequest::from_items(items, &csl_style, &locales));
    }

    let result = driver.finish(BibliographyRequest {
        style: &csl_style,
        locale: None,
        locale_files: &locales,
    });

    let mut output = Vec::new();
    if let Some(bib) = result.bibliography {
        for item in bib.items {
            let mut buf = String::new();
            item.content
                .write_buf(&mut buf, BufWriteFormat::Plain)
                .map_err(|e| e.to_string())?;
            output.push(buf);
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rotero_models::{Paper, Publication};

    fn test_paper(title: &str, authors: Vec<&str>, year: i32) -> Paper {
        let mut p = Paper::new(title.to_string());
        p.authors = authors.into_iter().map(String::from).collect();
        p.year = Some(year);
        p.publication = Publication {
            journal: Some("Nature".to_string()),
            volume: Some("1".to_string()),
            issue: None,
            pages: Some("1-10".to_string()),
            publisher: None,
        };
        p
    }

    #[test]
    fn test_format_inline_citations_single() {
        let papers = vec![test_paper(
            "Attention Is All You Need",
            vec!["Vaswani"],
            2017,
        )];
        let (individual, combined) =
            format_inline_citations(&papers, ArchivedStyle::AmericanPsychologicalAssociation)
                .unwrap();
        assert_eq!(individual.len(), 1);
        assert!(
            !individual[0].is_empty(),
            "inline citation should not be empty"
        );
        assert!(
            !combined.is_empty(),
            "combined citation should not be empty"
        );
        assert!(
            individual[0].contains("2017"),
            "should contain year: {}",
            individual[0]
        );
    }

    #[test]
    fn test_format_inline_citations_multiple() {
        let papers = vec![
            test_paper("Paper A", vec!["Smith"], 2020),
            test_paper("Paper B", vec!["Jones"], 2021),
        ];
        let (individual, combined) =
            format_inline_citations(&papers, ArchivedStyle::AmericanPsychologicalAssociation)
                .unwrap();
        assert_eq!(individual.len(), 2);
        assert!(
            combined.contains("2020"),
            "combined should have 2020: {combined}"
        );
        assert!(
            combined.contains("2021"),
            "combined should have 2021: {combined}"
        );
    }

    #[test]
    fn test_format_inline_citations_empty() {
        let (individual, combined) =
            format_inline_citations(&[], ArchivedStyle::AmericanPsychologicalAssociation).unwrap();
        assert!(individual.is_empty());
        assert!(combined.is_empty());
    }

    #[test]
    fn test_format_bibliography_entries_single() {
        let papers = vec![test_paper("Deep Learning", vec!["LeCun"], 2015)];
        let entries =
            format_bibliography_entries(&papers, ArchivedStyle::AmericanPsychologicalAssociation)
                .unwrap();
        assert_eq!(entries.len(), 1);
        assert!(!entries[0].is_empty());
        assert!(
            entries[0].contains("LeCun"),
            "should contain author: {}",
            entries[0]
        );
    }

    #[test]
    fn test_format_bibliography_entries_multiple() {
        let papers = vec![
            test_paper("Paper A", vec!["Smith"], 2020),
            test_paper("Paper B", vec!["Jones"], 2021),
        ];
        let entries =
            format_bibliography_entries(&papers, ArchivedStyle::AmericanPsychologicalAssociation)
                .unwrap();
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn test_format_bibliography_entries_empty() {
        let entries =
            format_bibliography_entries(&[], ArchivedStyle::AmericanPsychologicalAssociation)
                .unwrap();
        assert!(entries.is_empty());
    }
}
