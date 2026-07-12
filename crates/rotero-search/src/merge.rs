//! Deduplication and field-level merging of papers returned by multiple providers.
//!
//! When a fan-out search hits several providers, the same work often comes back
//! more than once with complementary gaps — arXiv may lack an abstract that
//! OpenAlex has, OpenAlex may lack the citation count Semantic Scholar reports.
//! [`dedupe_by_doi`] collapses same-DOI duplicates into one record, keeping the
//! most complete result and backfilling its remaining empty fields from the rest.

use std::collections::HashMap;

use rotero_models::Paper;

/// Fill empty fields of `primary` from `secondary`, leaving populated fields untouched.
///
/// Only additive: an already-present value on `primary` is never overwritten, so
/// the caller controls precedence by choosing which paper is primary.
pub fn merge_into(primary: &mut Paper, secondary: Paper) {
    if primary.abstract_text.is_none() {
        primary.abstract_text = secondary.abstract_text;
    }
    if primary.year.is_none() {
        primary.year = secondary.year;
    }
    if primary.citation.citation_count.is_none() {
        primary.citation.citation_count = secondary.citation.citation_count;
    }
    if primary.publication.journal.is_none() {
        primary.publication.journal = secondary.publication.journal;
    }
    if primary.links.pdf_url.is_none() {
        primary.links.pdf_url = secondary.links.pdf_url;
    }
    if primary.links.url.is_none() {
        primary.links.url = secondary.links.url;
    }
    if primary.creators.is_empty() {
        primary.creators = secondary.creators;
    }
}

/// Collapse duplicate papers by DOI, backfill-merging overlaps.
///
/// Papers sharing a DOI are folded into a single record: the most complete one
/// (by [`Paper::metadata_completeness_score`]) becomes primary and the others
/// backfill its empty fields via [`merge_into`]. Papers with no DOI are passed
/// through unchanged — they have no reliable dedup key. Input order is otherwise
/// preserved (first appearance of each DOI keeps its slot).
pub fn dedupe_by_doi(papers: Vec<Paper>) -> Vec<Paper> {
    // Index into `out` for each seen DOI, so we merge in place without reordering.
    let mut doi_slot: HashMap<String, usize> = HashMap::new();
    let mut out: Vec<Paper> = Vec::with_capacity(papers.len());

    for paper in papers {
        let doi = paper
            .doi
            .as_deref()
            .map(str::trim)
            .filter(|d| !d.is_empty());
        match doi {
            Some(doi) => match doi_slot.get(doi).copied() {
                Some(idx) => {
                    // Keep the more complete record as primary, backfill from the other.
                    if paper.metadata_completeness_score() > out[idx].metadata_completeness_score()
                    {
                        let mut winner = paper;
                        let loser = std::mem::take(&mut out[idx]);
                        merge_into(&mut winner, loser);
                        out[idx] = winner;
                    } else {
                        merge_into(&mut out[idx], paper);
                    }
                }
                None => {
                    doi_slot.insert(doi.to_string(), out.len());
                    out.push(paper);
                }
            },
            None => out.push(paper),
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paper_with(doi: &str, abstract_text: Option<&str>, year: Option<i32>) -> Paper {
        Paper {
            doi: Some(doi.to_string()),
            abstract_text: abstract_text.map(str::to_string),
            year,
            ..Default::default()
        }
    }

    #[test]
    fn backfills_missing_fields_across_duplicates() {
        // arXiv-style hit: has a year but no abstract. OpenAlex-style hit: has the abstract.
        let sparse = paper_with("10.1/x", None, Some(2020));
        let rich = paper_with("10.1/x", Some("the abstract"), None);
        let merged = dedupe_by_doi(vec![sparse, rich]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].abstract_text.as_deref(), Some("the abstract"));
        assert_eq!(merged[0].year, Some(2020));
    }

    #[test]
    fn keeps_no_doi_papers_separate() {
        let a = Paper {
            title: "A".into(),
            ..Default::default()
        };
        let b = Paper {
            title: "B".into(),
            ..Default::default()
        };
        assert_eq!(dedupe_by_doi(vec![a, b]).len(), 2);
    }

    #[test]
    fn empty_doi_is_not_a_dedup_key() {
        let a = paper_with("", Some("a"), None);
        let b = paper_with("   ", Some("b"), None);
        assert_eq!(dedupe_by_doi(vec![a, b]).len(), 2);
    }

    #[test]
    fn preserves_first_appearance_order() {
        let first = paper_with("10.1/a", None, None);
        let second = paper_with("10.1/b", None, None);
        let first_dup = paper_with("10.1/a", Some("late abstract"), None);
        let out = dedupe_by_doi(vec![first, second, first_dup]);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].doi.as_deref(), Some("10.1/a"));
        assert_eq!(out[0].abstract_text.as_deref(), Some("late abstract"));
        assert_eq!(out[1].doi.as_deref(), Some("10.1/b"));
    }
}
