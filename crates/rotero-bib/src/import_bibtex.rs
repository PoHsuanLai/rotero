use biblatex::{Bibliography, ChunksExt, PermissiveType};
use rotero_models::{Paper, PaperLinks, Publication};

/// A paper parsed from BibTeX, with an optional linked PDF path.
pub struct ImportedPaper {
    /// The parsed paper metadata.
    pub paper: Paper,
    /// Path to a PDF extracted from the BibTeX `file` field, if present.
    pub source_pdf: Option<String>,
}

/// Parses a BibTeX string and returns the extracted papers.
pub fn import_bibtex(input: &str) -> Result<Vec<ImportedPaper>, String> {
    let bibliography =
        Bibliography::parse(input).map_err(|e| format!("Failed to parse BibTeX: {e}"))?;

    let mut papers = Vec::new();

    for entry in bibliography.iter() {
        let title = entry
            .title()
            .map(|chunks| chunks.format_verbatim())
            .unwrap_or_default();

        let authors: Vec<String> = entry
            .author()
            .unwrap_or_default()
            .into_iter()
            .map(|person| {
                if person.given_name.is_empty() {
                    person.name.clone()
                } else {
                    format!("{} {}", person.given_name, person.name)
                }
            })
            .collect();

        let year = entry.date().ok().and_then(|d| match d {
            PermissiveType::Typed(date) => {
                let datetime = match date.value {
                    biblatex::DateValue::At(dt)
                    | biblatex::DateValue::After(dt)
                    | biblatex::DateValue::Before(dt) => dt,
                    biblatex::DateValue::Between(dt, _) => dt,
                };
                Some(datetime.year)
            }
            PermissiveType::Chunks(chunks) => {
                let s = chunks.format_verbatim();
                s.split('-').next().and_then(|y| y.parse::<i32>().ok())
            }
        });

        let journal = entry.journal().map(|chunks| chunks.format_verbatim()).ok();

        let volume = entry.volume().ok().map(|v| match v {
            PermissiveType::Typed(n) => n.to_string(),
            PermissiveType::Chunks(chunks) => chunks.format_verbatim(),
        });

        let issue = entry.number().map(|chunks| chunks.format_verbatim()).ok();

        let doi = entry.doi().ok();

        let url = entry.get("url").map(|chunks| chunks.format_verbatim());

        let pages = entry.pages().ok().map(|p| match p {
            PermissiveType::Typed(ranges) => ranges
                .iter()
                .map(|r| {
                    if r.start == r.end {
                        r.start.to_string()
                    } else {
                        format!("{}-{}", r.start, r.end)
                    }
                })
                .collect::<Vec<_>>()
                .join(", "),
            PermissiveType::Chunks(chunks) => chunks.format_verbatim(),
        });

        let abstract_text = entry
            .abstract_()
            .map(|chunks| chunks.format_verbatim())
            .ok();

        let publisher = entry.publisher().ok().map(|chunks_vec| {
            chunks_vec
                .iter()
                .map(|chunks| chunks.format_verbatim())
                .collect::<Vec<_>>()
                .join("; ")
        });

        // Extract first PDF path from the `file` field (Zotero format: "path1;path2;...")
        let source_pdf = entry.get("file").and_then(|chunks| {
            let raw = chunks.format_verbatim();
            raw.split(';')
                .map(|s| s.trim())
                .find(|s| s.to_lowercase().ends_with(".pdf"))
                .map(|s| s.to_string())
        });

        papers.push(ImportedPaper {
            source_pdf,
            paper: Paper {
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
            },
        });
    }

    Ok(papers)
}
