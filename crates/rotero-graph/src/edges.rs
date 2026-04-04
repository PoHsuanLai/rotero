use std::collections::HashMap;

use rotero_models::{Paper, Tag};

use crate::data::{EdgeType, GraphFilter};

/// A raw edge before deduplication.
#[derive(Debug)]
struct RawEdge {
    source: i64,
    target: i64,
    rel_type: EdgeType,
    label: String,
}

/// Merged edge between two papers (may combine multiple shared attributes).
#[derive(Debug, Clone)]
pub struct MergedEdge {
    pub source: i64,
    pub target: i64,
    pub rel_type: EdgeType,
    pub label: String,
    pub weight: f32,
}

/// Compute all edges between papers based on shared attributes.
pub fn compute_edges(
    papers: &[Paper],
    tags: &[Tag],
    paper_tag_pairs: &[(i64, i64)],
    paper_collection_pairs: &[(i64, i64)],
    filter: &GraphFilter,
) -> Vec<MergedEdge> {
    let tag_name_map: HashMap<i64, &str> = tags
        .iter()
        .filter_map(|t| Some((t.id?, t.name.as_str())))
        .collect();

    let paper_ids: std::collections::HashSet<i64> = papers
        .iter()
        .filter_map(|p| p.id)
        .collect();

    let mut raw_edges = Vec::new();

    // Shared tags
    if filter.show_tag_edges {
        let mut tag_to_papers: HashMap<i64, Vec<i64>> = HashMap::new();
        for &(paper_id, tag_id) in paper_tag_pairs {
            if paper_ids.contains(&paper_id) {
                tag_to_papers.entry(tag_id).or_default().push(paper_id);
            }
        }
        for (tag_id, pids) in &tag_to_papers {
            let label = tag_name_map
                .get(tag_id)
                .unwrap_or(&"tag")
                .to_string();
            add_pairwise_edges(&mut raw_edges, pids, EdgeType::Tag, &label);
        }
    }

    // Shared collections
    if filter.show_collection_edges {
        let mut coll_to_papers: HashMap<i64, Vec<i64>> = HashMap::new();
        for &(paper_id, coll_id) in paper_collection_pairs {
            if paper_ids.contains(&paper_id) {
                coll_to_papers.entry(coll_id).or_default().push(paper_id);
            }
        }
        for (_coll_id, pids) in &coll_to_papers {
            add_pairwise_edges(&mut raw_edges, pids, EdgeType::Collection, "collection");
        }
    }

    // Shared authors
    if filter.show_author_edges {
        let mut author_to_papers: HashMap<String, Vec<i64>> = HashMap::new();
        for paper in papers {
            if let Some(pid) = paper.id {
                for author in &paper.authors {
                    let key = author.trim().to_lowercase();
                    if !key.is_empty() {
                        author_to_papers.entry(key).or_default().push(pid);
                    }
                }
            }
        }
        for (author, pids) in &author_to_papers {
            if pids.len() > filter.max_author_group_size {
                continue; // Skip prolific authors
            }
            add_pairwise_edges(&mut raw_edges, pids, EdgeType::Author, author);
        }
    }

    // Shared journal
    if filter.show_journal_edges {
        let mut journal_to_papers: HashMap<String, Vec<i64>> = HashMap::new();
        for paper in papers {
            if let Some(pid) = paper.id {
                if let Some(ref j) = paper.journal {
                    let key = j.trim().to_lowercase();
                    if !key.is_empty() {
                        journal_to_papers.entry(key).or_default().push(pid);
                    }
                }
            }
        }
        for (journal, pids) in &journal_to_papers {
            if pids.len() > filter.max_author_group_size {
                continue;
            }
            add_pairwise_edges(&mut raw_edges, pids, EdgeType::Journal, journal);
        }
    }

    merge_edges(raw_edges, filter.max_edges_per_node)
}

/// Generate pairwise edges for a group of papers sharing an attribute.
fn add_pairwise_edges(
    edges: &mut Vec<RawEdge>,
    paper_ids: &[i64],
    rel_type: EdgeType,
    label: &str,
) {
    for i in 0..paper_ids.len() {
        for j in (i + 1)..paper_ids.len() {
            let (a, b) = if paper_ids[i] < paper_ids[j] {
                (paper_ids[i], paper_ids[j])
            } else {
                (paper_ids[j], paper_ids[i])
            };
            edges.push(RawEdge {
                source: a,
                target: b,
                rel_type,
                label: label.to_string(),
            });
        }
    }
}

/// Merge raw edges between the same paper pair, summing weights.
/// Then cap edges per node.
fn merge_edges(raw: Vec<RawEdge>, max_per_node: usize) -> Vec<MergedEdge> {
    // Group by (source, target) — pick the strongest edge type, sum weight
    let mut map: HashMap<(i64, i64), MergedEdge> = HashMap::new();

    for e in raw {
        let key = (e.source, e.target);
        map.entry(key)
            .and_modify(|existing| {
                existing.weight += 1.0;
                // Keep the more specific label (tag > collection > author)
                if edge_type_priority(e.rel_type) > edge_type_priority(existing.rel_type) {
                    existing.rel_type = e.rel_type;
                    existing.label = e.label.clone();
                }
            })
            .or_insert(MergedEdge {
                source: e.source,
                target: e.target,
                rel_type: e.rel_type,
                label: e.label,
                weight: 1.0,
            });
    }

    let mut edges: Vec<MergedEdge> = map.into_values().collect();

    // Sort by weight descending for capping
    edges.sort_by(|a, b| b.weight.partial_cmp(&a.weight).unwrap_or(std::cmp::Ordering::Equal));

    // Cap edges per node
    let mut node_edge_count: HashMap<i64, usize> = HashMap::new();
    edges.retain(|e| {
        let src_count = node_edge_count.entry(e.source).or_insert(0);
        if *src_count >= max_per_node {
            return false;
        }
        let tgt_count = node_edge_count.entry(e.target).or_insert(0);
        if *tgt_count >= max_per_node {
            return false;
        }
        *node_edge_count.get_mut(&e.source).unwrap() += 1;
        *node_edge_count.get_mut(&e.target).unwrap() += 1;
        true
    });

    edges
}

fn edge_type_priority(t: EdgeType) -> u8 {
    match t {
        EdgeType::Tag => 3,
        EdgeType::Collection => 2,
        EdgeType::Author => 1,
        EdgeType::Journal => 0,
    }
}
