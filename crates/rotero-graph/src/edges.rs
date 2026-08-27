use std::collections::HashMap;

use rotero_models::{Paper, Tag};

use crate::data::{EdgeType, GraphFilter, Relations};

/// A raw edge before deduplication.
#[derive(Debug)]
struct RawEdge {
    source: String,
    target: String,
    rel_type: EdgeType,
    label: String,
}

/// Merged edge between two papers (may combine multiple shared attributes).
#[derive(Debug, Clone)]
pub struct MergedEdge {
    pub source: String,
    pub target: String,
    pub rel_type: EdgeType,
    pub label: String,
    pub weight: f32,
}

/// Compute pairwise edges between papers based on shared tags, collections,
/// authors, and journals, then merge duplicates and cap per-node edge count.
pub fn compute_edges(
    papers: &[Paper],
    tags: &[Tag],
    relations: Relations<'_>,
    filter: &GraphFilter,
) -> Vec<MergedEdge> {
    let Relations {
        paper_tags: paper_tag_pairs,
        paper_collections: paper_collection_pairs,
        citations: citation_pairs,
        conversations: conversation_pairs,
    } = relations;

    let tag_name_map: HashMap<&str, &str> = tags
        .iter()
        .filter_map(|t| Some((t.id.as_deref()?, t.name.as_str())))
        .collect();

    let paper_ids: std::collections::HashSet<&str> =
        papers.iter().filter_map(|p| p.id.as_deref()).collect();

    let mut raw_edges = Vec::new();

    // Shared tags
    if filter.show_tag_edges {
        let mut tag_to_papers: HashMap<&str, Vec<&str>> = HashMap::new();
        for (paper_id, tag_id) in paper_tag_pairs {
            if paper_ids.contains(paper_id.as_str()) {
                tag_to_papers
                    .entry(tag_id.as_str())
                    .or_default()
                    .push(paper_id.as_str());
            }
        }
        for (tag_id, pids) in &tag_to_papers {
            let label = tag_name_map.get(tag_id).unwrap_or(&"tag").to_string();
            add_pairwise_edges(&mut raw_edges, pids, EdgeType::Tag, &label);
        }
    }

    // Shared collections
    if filter.show_collection_edges {
        let mut coll_to_papers: HashMap<&str, Vec<&str>> = HashMap::new();
        for (paper_id, coll_id) in paper_collection_pairs {
            if paper_ids.contains(paper_id.as_str()) {
                coll_to_papers
                    .entry(coll_id.as_str())
                    .or_default()
                    .push(paper_id.as_str());
            }
        }
        for pids in coll_to_papers.values() {
            add_pairwise_edges(&mut raw_edges, pids, EdgeType::Collection, "collection");
        }
    }

    // Shared authors
    if filter.show_author_edges {
        let mut author_to_papers: HashMap<String, Vec<&str>> = HashMap::new();
        for paper in papers {
            if let Some(ref pid) = paper.id {
                for author in paper.author_names() {
                    let key = author.trim().to_lowercase();
                    if !key.is_empty() {
                        author_to_papers.entry(key).or_default().push(pid.as_str());
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
        let mut journal_to_papers: HashMap<String, Vec<&str>> = HashMap::new();
        for paper in papers {
            if let Some(ref pid) = paper.id
                && let Some(ref j) = paper.publication.journal
            {
                let key = j.trim().to_lowercase();
                if !key.is_empty() {
                    journal_to_papers.entry(key).or_default().push(pid.as_str());
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

    // Citations — directed (citing → cited). Unlike the shared-attribute edges
    // above, these are NOT normalized by id order, so A→B and B→A stay distinct.
    if filter.show_citation_edges {
        for (citing, cited) in citation_pairs {
            if citing != cited
                && paper_ids.contains(citing.as_str())
                && paper_ids.contains(cited.as_str())
            {
                raw_edges.push(RawEdge {
                    source: citing.clone(),
                    target: cited.clone(),
                    rel_type: EdgeType::Citation,
                    label: "cites".to_string(),
                });
            }
        }
    }

    // Papers discussed together in one conversation. Grouped like shared tags:
    // the pairs arrive as (session_id, paper_id), so every paper a single
    // conversation is *about* links to every other. Papers the agent merely
    // read are not in this list — only subjects are — so a library search the
    // agent ran does not wire its results together.
    if filter.show_conversation_edges {
        let mut session_to_papers: HashMap<&str, Vec<&str>> = HashMap::new();
        for (session_id, paper_id) in conversation_pairs {
            if paper_ids.contains(paper_id.as_str()) {
                session_to_papers
                    .entry(session_id.as_str())
                    .or_default()
                    .push(paper_id.as_str());
            }
        }
        for pids in session_to_papers.values() {
            add_pairwise_edges(
                &mut raw_edges,
                pids,
                EdgeType::Conversation,
                "discussed together",
            );
        }
    }

    merge_edges(raw_edges, filter.max_edges_per_node)
}

fn add_pairwise_edges(
    edges: &mut Vec<RawEdge>,
    paper_ids: &[&str],
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
                source: a.to_string(),
                target: b.to_string(),
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
    let mut map: HashMap<(String, String), MergedEdge> = HashMap::new();

    for e in raw {
        let key = (e.source.clone(), e.target.clone());
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

    edges.sort_by(|a, b| {
        b.weight
            .partial_cmp(&a.weight)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut node_edge_count: HashMap<String, usize> = HashMap::new();
    edges.retain(|e| {
        let src_count = node_edge_count.entry(e.source.clone()).or_insert(0);
        if *src_count >= max_per_node {
            return false;
        }
        let tgt_count = node_edge_count.entry(e.target.clone()).or_insert(0);
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
        // A conversation outranks everything: the user chose these papers and
        // discussed them together, which no derived metadata match can beat.
        EdgeType::Conversation => 5,
        // Citations are the next strongest — an explicit A→B reference — so if
        // a pair also shares metadata, the citation label wins.
        EdgeType::Citation => 4,
        EdgeType::Tag => 3,
        EdgeType::Collection => 2,
        EdgeType::Author => 1,
        EdgeType::Journal => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::GraphFilter;

    fn paper(id: &str) -> Paper {
        Paper {
            id: Some(id.to_string()),
            ..Default::default()
        }
    }

    fn conversation_only_filter() -> GraphFilter {
        GraphFilter {
            show_tag_edges: false,
            show_collection_edges: false,
            show_author_edges: false,
            show_journal_edges: false,
            show_conversation_edges: true,
            ..Default::default()
        }
    }

    /// `(session_id, paper_id)`, the shape `all_chat_session_subjects` returns.
    fn chat(session: &str, paper: &str) -> (String, String) {
        (session.to_string(), paper.to_string())
    }

    #[test]
    fn papers_discussed_in_one_conversation_are_linked() {
        let papers = [paper("a"), paper("b"), paper("c")];
        let chats = [chat("s1", "a"), chat("s1", "b"), chat("s1", "c")];
        let edges = compute_edges(
            &papers,
            &[],
            Relations {
                conversations: &chats,
                ..Default::default()
            },
            &conversation_only_filter(),
        );
        // A three-paper conversation is a triangle, not a star.
        assert_eq!(edges.len(), 3);
        assert!(edges.iter().all(|e| e.rel_type == EdgeType::Conversation));
    }

    /// The point of the mode: a conversation about one paper is real history,
    /// but it has no second paper to link to and so draws no edge. The node
    /// marker is what carries it — see `is_discussed` in the crate root.
    #[test]
    fn a_conversation_about_one_paper_draws_no_edge() {
        let papers = [paper("a"), paper("b")];
        let chats = [chat("s1", "a")];
        let edges = compute_edges(
            &papers,
            &[],
            Relations {
                conversations: &chats,
                ..Default::default()
            },
            &conversation_only_filter(),
        );
        assert!(edges.is_empty());
    }

    #[test]
    fn separate_conversations_do_not_link_their_papers() {
        let papers = [paper("a"), paper("b")];
        let chats = [chat("s1", "a"), chat("s2", "b")];
        let edges = compute_edges(
            &papers,
            &[],
            Relations {
                conversations: &chats,
                ..Default::default()
            },
            &conversation_only_filter(),
        );
        assert!(edges.is_empty());
    }

    /// Two conversations covering the same pair are one edge, weighted heavier.
    #[test]
    fn discussing_a_pair_twice_strengthens_one_edge() {
        let papers = [paper("a"), paper("b")];
        let chats = [
            chat("s1", "a"),
            chat("s1", "b"),
            chat("s2", "a"),
            chat("s2", "b"),
        ];
        let edges = compute_edges(
            &papers,
            &[],
            Relations {
                conversations: &chats,
                ..Default::default()
            },
            &conversation_only_filter(),
        );
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].weight, 2.0);
    }

    #[test]
    fn a_deleted_paper_drops_out_of_conversation_edges() {
        let papers = [paper("a")];
        let chats = [chat("s1", "a"), chat("s1", "gone")];
        let edges = compute_edges(
            &papers,
            &[],
            Relations {
                conversations: &chats,
                ..Default::default()
            },
            &conversation_only_filter(),
        );
        assert!(edges.is_empty());
    }

    #[test]
    fn conversations_hidden_when_filter_off() {
        let papers = [paper("a"), paper("b")];
        let chats = [chat("s1", "a"), chat("s1", "b")];
        let edges = compute_edges(
            &papers,
            &[],
            Relations {
                conversations: &chats,
                ..Default::default()
            },
            &GraphFilter::default(),
        );
        assert!(edges.is_empty());
    }

    /// A pair that both shares a tag and was discussed together is labelled by
    /// the conversation: the user's own grouping outranks derived metadata.
    #[test]
    fn a_conversation_outranks_a_shared_tag() {
        let papers = [paper("a"), paper("b")];
        let tag_pairs = [
            ("a".to_string(), "t1".to_string()),
            ("b".to_string(), "t1".to_string()),
        ];
        let chats = [chat("s1", "a"), chat("s1", "b")];
        let filter = GraphFilter {
            show_tag_edges: true,
            show_conversation_edges: true,
            ..conversation_only_filter()
        };
        let edges = compute_edges(
            &papers,
            &[],
            Relations {
                paper_tags: &tag_pairs,
                conversations: &chats,
                ..Default::default()
            },
            &filter,
        );
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].rel_type, EdgeType::Conversation);
    }

    fn citation_only_filter() -> GraphFilter {
        GraphFilter {
            show_tag_edges: false,
            show_collection_edges: false,
            show_author_edges: false,
            show_journal_edges: false,
            show_citation_edges: true,
            ..Default::default()
        }
    }

    #[test]
    fn citation_edges_are_directed() {
        let papers = [paper("a"), paper("b")];
        let cites = [("a".to_string(), "b".to_string())];
        let edges = compute_edges(
            &papers,
            &[],
            Relations {
                citations: &cites,
                ..Default::default()
            },
            &citation_only_filter(),
        );
        assert_eq!(edges.len(), 1);
        // Preserved as citing → cited, NOT normalized by id order.
        assert_eq!(edges[0].source, "a");
        assert_eq!(edges[0].target, "b");
        assert_eq!(edges[0].rel_type, EdgeType::Citation);
    }

    #[test]
    fn opposite_directions_are_distinct_edges() {
        let papers = [paper("a"), paper("b")];
        let cites = [
            ("a".to_string(), "b".to_string()),
            ("b".to_string(), "a".to_string()),
        ];
        let edges = compute_edges(
            &papers,
            &[],
            Relations {
                citations: &cites,
                ..Default::default()
            },
            &citation_only_filter(),
        );
        assert_eq!(edges.len(), 2);
    }

    #[test]
    fn citations_hidden_when_filter_off() {
        let papers = [paper("a"), paper("b")];
        let cites = [("a".to_string(), "b".to_string())];
        let edges = compute_edges(
            &papers,
            &[],
            Relations {
                citations: &cites,
                ..Default::default()
            },
            &GraphFilter::default(),
        );
        assert!(edges.is_empty());
    }

    #[test]
    fn dangling_and_self_citations_are_skipped() {
        let papers = [paper("a")];
        let cites = [
            ("a".to_string(), "a".to_string()),       // self-citation
            ("a".to_string(), "missing".to_string()), // target not in library
        ];
        let edges = compute_edges(
            &papers,
            &[],
            Relations {
                citations: &cites,
                ..Default::default()
            },
            &citation_only_filter(),
        );
        assert!(edges.is_empty());
    }
}
