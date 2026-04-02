use rotero_models::Paper;

use super::crossref::FetchedMetadata;

/// Convert CrossRef metadata into a Paper model.
pub fn metadata_to_paper(meta: FetchedMetadata) -> Paper {
    let mut paper = Paper::new(meta.title);
    paper.authors = meta.authors;
    paper.year = meta.year;
    paper.doi = Some(meta.doi);
    paper.abstract_text = meta.abstract_text;
    paper.journal = meta.journal;
    paper.volume = meta.volume;
    paper.issue = meta.issue;
    paper.pages = meta.pages;
    paper.publisher = meta.publisher;
    paper.url = meta.url;
    paper
}
