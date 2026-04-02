use rotero_models::Paper;

/// Export a list of Papers to BibTeX format.
pub fn export_bibtex(papers: &[Paper]) -> String {
    let mut output = String::new();

    for paper in papers {
        let key = generate_cite_key(paper);
        output.push_str(&format!("@article{{{key},\n"));

        output.push_str(&format!("  title = {{{{{}}}}},\n", paper.title));

        if !paper.authors.is_empty() {
            let authors_str = paper.authors.join(" and ");
            output.push_str(&format!("  author = {{{{{authors_str}}}}},\n"));
        }

        if let Some(year) = paper.year {
            output.push_str(&format!("  year = {{{year}}},\n"));
        }

        if let Some(ref journal) = paper.journal {
            output.push_str(&format!("  journal = {{{{{journal}}}}},\n"));
        }

        if let Some(ref volume) = paper.volume {
            output.push_str(&format!("  volume = {{{volume}}},\n"));
        }

        if let Some(ref issue) = paper.issue {
            output.push_str(&format!("  number = {{{issue}}},\n"));
        }

        if let Some(ref pages) = paper.pages {
            output.push_str(&format!("  pages = {{{pages}}},\n"));
        }

        if let Some(ref doi) = paper.doi {
            output.push_str(&format!("  doi = {{{doi}}},\n"));
        }

        if let Some(ref url) = paper.url {
            output.push_str(&format!("  url = {{{url}}},\n"));
        }

        if let Some(ref publisher) = paper.publisher {
            output.push_str(&format!("  publisher = {{{{{publisher}}}}},\n"));
        }

        if let Some(ref abstract_text) = paper.abstract_text {
            output.push_str(&format!("  abstract = {{{{{abstract_text}}}}},\n"));
        }

        output.push_str("}\n\n");
    }

    output
}

/// Generate a citation key from paper metadata.
fn generate_cite_key(paper: &Paper) -> String {
    let author_part = paper
        .authors
        .first()
        .map(|a| {
            a.split_whitespace()
                .last()
                .unwrap_or("unknown")
                .to_lowercase()
        })
        .unwrap_or_else(|| "unknown".to_string());

    let year_part = paper
        .year
        .map(|y| y.to_string())
        .unwrap_or_else(|| "nd".to_string());

    let title_word = paper
        .title
        .split_whitespace()
        .find(|w| w.len() > 3)
        .map(|w| w.to_lowercase())
        .unwrap_or_else(|| "paper".to_string());

    format!("{author_part}{year_part}{title_word}")
}
