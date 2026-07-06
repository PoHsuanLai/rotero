//! Merge, deduplicate, and rank web-search results across providers.
//!
//! Each provider (OpenAlex, arXiv, Semantic Scholar) returns its own list of
//! [`Paper`]s for a query. The same paper commonly appears in several of them,
//! so [`merge_and_rank`] collapses duplicates into a single richer paper and
//! orders the result by a blended relevance score.

use std::collections::HashMap;

use crate::paper::{Paper, PaperId, normalize_title};

/// Key used to detect that two results describe the same paper.
///
/// Prefers the canonical [`PaperId`] (which collapses `arXiv:X` and
/// `10.48550/arXiv.X` to the same value), falling back to the normalized title,
/// and finally to a per-input-index unique key for title-less results so they
/// are never merged together by accident.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum DedupKey {
    Id(PaperId),
    Title(String),
    Unique(usize),
}

fn dedup_key(paper: &Paper, index: usize) -> DedupKey {
    if let Some(pid) = paper.paper_id() {
        return DedupKey::Id(pid);
    }
    let nt = normalize_title(&paper.title);
    if nt.is_empty() {
        DedupKey::Unique(index)
    } else {
        DedupKey::Title(nt)
    }
}

/// Whether a DOI string is a real publisher DOI rather than arXiv's stored
/// `arXiv:ID` pseudo-DOI.
fn is_real_doi(doi: &Option<String>) -> bool {
    matches!(
        doi.as_deref().and_then(PaperId::parse),
        Some(PaperId::Doi(_))
    )
}

/// Merge `other` into `keep`, filling gaps and keeping the strongest signal from
/// each. `keep` is expected to be the more metadata-complete of the two.
fn merge_into(keep: &mut Paper, other: Paper) {
    if keep.abstract_text.is_none() {
        keep.abstract_text = other.abstract_text;
    }
    if keep.year.is_none() {
        keep.year = other.year;
    }
    if keep.authors.is_empty() {
        keep.authors = other.authors;
    }
    if keep.publication.journal.is_none() {
        keep.publication = other.publication;
    }
    if keep.links.pdf_url.is_none() {
        keep.links.pdf_url = other.links.pdf_url;
    }
    if keep.links.url.is_none() {
        keep.links.url = other.links.url;
    }

    // Prefer a real publisher DOI over arXiv's `arXiv:ID` pseudo-DOI, and fill
    // in a missing DOI from the other source.
    let keep_missing = keep.doi.as_deref().unwrap_or("").is_empty();
    let upgrade_to_real = !is_real_doi(&keep.doi) && is_real_doi(&other.doi);
    if keep_missing || upgrade_to_real {
        keep.doi = other.doi;
    }

    // Citation count: keep the larger.
    keep.citation.citation_count = keep
        .citation
        .citation_count
        .max(other.citation.citation_count);
}

/// Normalize a single provider's batch of results to a [0, 1] relevance value,
/// so scores from different providers (whose raw scales differ by orders of
/// magnitude) become comparable. Returns `norm` keyed by the paper's index in
/// `papers`.
///
/// - If any result carries a raw API score, min-max normalize the raw scores.
/// - Otherwise fall back to rank position (`1 - i/len`), so the top result ≈ 1.0.
fn normalize_batch(papers: &[Paper]) -> Vec<f64> {
    let len = papers.len();
    if len == 0 {
        return Vec::new();
    }

    let raw: Vec<Option<f64>> = papers
        .iter()
        .map(|p| p.search_rank.and_then(|r| r.raw_score))
        .collect();

    let scored: Vec<f64> = raw.iter().filter_map(|r| *r).collect();
    if scored.is_empty() {
        // No raw scores in this batch — derive from rank position.
        return (0..len).map(|i| 1.0 - (i as f64) / (len as f64)).collect();
    }

    let min = scored.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = scored.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let span = max - min;

    raw.iter()
        .enumerate()
        .map(|(i, r)| match r {
            Some(v) if span > 0.0 => (v - min) / span,
            Some(_) => 1.0, // all raw scores equal → treat as top relevance
            None => 1.0 - (i as f64) / (len as f64), // mixed batch: fall back to rank
        })
        .collect()
}

/// A merged paper plus the ranking signals accumulated across its sources.
struct Merged {
    paper: Paper,
    /// Best (max) normalized relevance across contributing sources.
    norm_relevance: f64,
    /// Number of distinct providers that returned this paper.
    source_count: u32,
}

/// Merge all providers' results into one deduplicated, relevance-ranked list.
///
/// `provider_results` is the per-provider batches in a fixed order. Each batch
/// is normalized independently, then results are folded on [`DedupKey`]; the
/// list is scored against `query` and sorted best-first.
pub fn merge_and_rank(provider_results: &[Vec<Paper>], query: &str) -> Vec<Paper> {
    let nq = normalize_title(query);
    let query_tokens: Vec<&str> = nq.split_whitespace().collect();

    let mut map: HashMap<DedupKey, Merged> = HashMap::new();
    // Preserve first-seen order for stable output before the final sort.
    let mut order: Vec<DedupKey> = Vec::new();
    let mut global_index = 0usize;

    for batch in provider_results {
        let norms = normalize_batch(batch);
        for (i, paper) in batch.iter().enumerate() {
            let key = dedup_key(paper, global_index);
            global_index += 1;
            let norm = norms.get(i).copied().unwrap_or(0.0);

            match map.get_mut(&key) {
                Some(existing) => {
                    existing.norm_relevance = existing.norm_relevance.max(norm);
                    existing.source_count += 1;
                    // Keep the more complete paper as the base.
                    if paper.metadata_completeness_score()
                        > existing.paper.metadata_completeness_score()
                    {
                        let mut base = paper.clone();
                        merge_into(&mut base, std::mem::take(&mut existing.paper));
                        existing.paper = base;
                    } else {
                        merge_into(&mut existing.paper, paper.clone());
                    }
                }
                None => {
                    order.push(key.clone());
                    map.insert(
                        key,
                        Merged {
                            paper: paper.clone(),
                            norm_relevance: norm,
                            source_count: 1,
                        },
                    );
                }
            }
        }
    }

    let mut merged: Vec<Merged> = order.into_iter().filter_map(|k| map.remove(&k)).collect();

    // Sort by blended score, best first. Ties fall back to normalized relevance,
    // then citation count, then title for a deterministic order.
    merged.sort_by(|a, b| {
        let sa = score(a, &nq, &query_tokens);
        let sb = score(b, &nq, &query_tokens);
        sb.partial_cmp(&sa)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(
                b.norm_relevance
                    .partial_cmp(&a.norm_relevance)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
            .then(
                b.paper
                    .citation
                    .citation_count
                    .cmp(&a.paper.citation.citation_count),
            )
            .then(a.paper.title.cmp(&b.paper.title))
    });

    merged.into_iter().map(|m| m.paper).collect()
}

/// Blend the ranking signals into a single score (higher = more relevant).
/// Exact/prefix title match dominates so the searched-for paper lands first;
/// below that, normalized API relevance orders the list.
fn score(m: &Merged, nq: &str, query_tokens: &[&str]) -> f64 {
    let nt = normalize_title(&m.paper.title);
    let mut s = 0.0;

    if !nq.is_empty() {
        if nt == nq {
            s += 1000.0;
        } else if nt.starts_with(nq) {
            s += 400.0;
        }
    }

    // Fraction of query tokens present in the title.
    if !query_tokens.is_empty() {
        let title_tokens: Vec<&str> = nt.split_whitespace().collect();
        let hits = query_tokens
            .iter()
            .filter(|t| title_tokens.contains(t))
            .count();
        s += 200.0 * (hits as f64) / (query_tokens.len() as f64);
    }

    s += 300.0 * m.norm_relevance;

    let citations = m.paper.citation.citation_count.unwrap_or(0).max(0) as f64;
    s += 3.0 * (1.0 + citations).ln();

    s += 10.0 * ((m.source_count.saturating_sub(1)) as f64);

    if let Some(year) = m.paper.year {
        s += 0.2 * ((year - 2000).clamp(0, 25) as f64);
    }

    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paper::{ProviderKind, SearchRank};

    fn paper(title: &str, doi: Option<&str>) -> Paper {
        Paper {
            title: title.to_string(),
            doi: doi.map(|d| d.to_string()),
            ..Default::default()
        }
    }

    fn with_rank(mut p: Paper, source: ProviderKind, raw: Option<f64>, pos: usize) -> Paper {
        p.search_rank = Some(SearchRank {
            source,
            raw_score: raw,
            position: pos,
        });
        p
    }

    #[test]
    fn dedups_same_paper_across_sources() {
        // arXiv stores the pseudo-DOI; OpenAlex/S2 carry the arXiv DOI form and a real one.
        let arxiv = with_rank(
            paper("Attention Is All You Need", Some("arXiv:1706.03762")),
            ProviderKind::ArXiv,
            None,
            0,
        );
        let openalex = {
            let mut p = with_rank(
                paper(
                    "Attention Is All You Need",
                    Some("10.48550/arXiv.1706.03762"),
                ),
                ProviderKind::OpenAlex,
                Some(850.0),
                0,
            );
            p.citation.citation_count = Some(100000);
            p.authors = vec!["Vaswani".into()];
            p
        };
        // Semantic Scholar returns the same paper by its arXiv DOI (the common
        // case for a preprint) — canonicalizes to the same PaperId::ArXiv.
        let s2 = with_rank(
            paper(
                "Attention Is All You Need",
                Some("10.48550/arXiv.1706.03762"),
            ),
            ProviderKind::SemanticScholar,
            Some(0.9),
            0,
        );
        // An unrelated paper that must NOT be merged in.
        let other = with_rank(
            paper("A Completely Different Paper", Some("10.9/other")),
            ProviderKind::OpenAlex,
            Some(300.0),
            1,
        );

        let out = merge_and_rank(
            &[vec![arxiv], vec![openalex, other], vec![s2]],
            "attention is all you need",
        );

        // The three copies of the transformer paper collapse into one; the
        // unrelated paper stays separate.
        assert_eq!(out.len(), 2);
        let p = out
            .iter()
            .find(|p| p.title == "Attention Is All You Need")
            .expect("merged paper present");
        // Citation count survives from OpenAlex.
        assert_eq!(p.citation.citation_count, Some(100000));
        // Authors filled from the richer source.
        assert_eq!(p.authors, vec!["Vaswani".to_string()]);
        // The exact-title paper ranks first.
        assert_eq!(out[0].title, "Attention Is All You Need");
    }

    #[test]
    fn normalization_makes_scales_comparable() {
        // OpenAlex raw scores on one scale, S2 on a totally different one.
        let oa = vec![
            with_rank(
                paper("A", Some("10.1/a")),
                ProviderKind::OpenAlex,
                Some(850.0),
                0,
            ),
            with_rank(
                paper("B", Some("10.1/b")),
                ProviderKind::OpenAlex,
                Some(12.0),
                1,
            ),
        ];
        let s2 = vec![
            with_rank(
                paper("C", Some("10.2/c")),
                ProviderKind::SemanticScholar,
                Some(0.9),
                0,
            ),
            with_rank(
                paper("D", Some("10.2/d")),
                ProviderKind::SemanticScholar,
                Some(0.1),
                1,
            ),
        ];
        let oa_norm = normalize_batch(&oa);
        let s2_norm = normalize_batch(&s2);
        // Each source's top hit normalizes to ~1.0 and weakest to ~0.0.
        assert!((oa_norm[0] - 1.0).abs() < 1e-9);
        assert!((oa_norm[1] - 0.0).abs() < 1e-9);
        assert!((s2_norm[0] - 1.0).abs() < 1e-9);
        assert!((s2_norm[1] - 0.0).abs() < 1e-9);
    }

    #[test]
    fn arxiv_rank_derived_norm() {
        let batch = vec![
            with_rank(paper("A", Some("arXiv:1")), ProviderKind::ArXiv, None, 0),
            with_rank(paper("B", Some("arXiv:2")), ProviderKind::ArXiv, None, 1),
            with_rank(paper("C", Some("arXiv:3")), ProviderKind::ArXiv, None, 2),
        ];
        let norm = normalize_batch(&batch);
        assert!((norm[0] - 1.0).abs() < 1e-9);
        assert!(norm[0] > norm[1] && norm[1] > norm[2]);
    }

    #[test]
    fn exact_title_beats_higher_cited_partial() {
        // Partial match with huge citation count...
        let mut partial = with_rank(
            paper(
                "Attention and Memory in Deep Learning",
                Some("10.1/partial"),
            ),
            ProviderKind::OpenAlex,
            Some(500.0),
            0,
        );
        partial.citation.citation_count = Some(999999);
        // ...vs the exact title with modest citations.
        let mut exact = with_rank(
            paper("Attention Is All You Need", Some("10.1/exact")),
            ProviderKind::OpenAlex,
            Some(400.0),
            1,
        );
        exact.citation.citation_count = Some(10);

        let out = merge_and_rank(&[vec![partial, exact]], "attention is all you need");
        assert_eq!(out[0].title, "Attention Is All You Need");
    }
}
