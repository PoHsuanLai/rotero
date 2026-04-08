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
