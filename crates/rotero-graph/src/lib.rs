pub mod data;
pub mod edges;

use std::collections::HashMap;

use fdg::{Force, fruchterman_reingold::FruchtermanReingold, simple::Center};
use petgraph::stable_graph::StableGraph;
use rotero_models::{Paper, Tag};

pub use data::{EdgeType, GraphData, GraphEdge, GraphFilter, GraphNode};
pub use edges::MergedEdge;

/// Build the full graph and run force simulation.
pub fn build_and_simulate(
    papers: &[Paper],
    tags: &[Tag],
    paper_tag_pairs: &[(String, String)],
    paper_collection_pairs: &[(String, String)],
    filter: &GraphFilter,
    iterations: usize,
) -> GraphData {
    let merged_edges = edges::compute_edges(
        papers,
        tags,
        paper_tag_pairs,
        paper_collection_pairs,
        filter,
    );

    let tag_colors: HashMap<&str, &str> = tags
        .iter()
        .filter_map(|t| Some((t.id.as_deref()?, t.color.as_deref()?)))
        .collect();

    // Build paper -> first tag color lookup
    let mut paper_tag_color: HashMap<&str, String> = HashMap::new();
    for (paper_id, tag_id) in paper_tag_pairs {
        if !paper_tag_color.contains_key(paper_id.as_str()) {
            if let Some(&color) = tag_colors.get(tag_id.as_str()) {
                paper_tag_color.insert(paper_id.as_str(), color.to_string());
            }
        }
    }

    // Build petgraph — use directed (default) since fdg expects Into<StableGraph<N, E>>
    // Direction doesn't affect force layout.
    let mut id_to_idx: HashMap<&str, petgraph::graph::NodeIndex> = HashMap::new();
    let mut source_graph: StableGraph<String, f32> = StableGraph::new();

    for paper in papers {
        if let Some(ref pid) = paper.id {
            let idx = source_graph.add_node(pid.clone());
            id_to_idx.insert(pid.as_str(), idx);
        }
    }

    for edge in &merged_edges {
        if let (Some(&src), Some(&tgt)) = (
            id_to_idx.get(edge.source.as_str()),
            id_to_idx.get(edge.target.as_str()),
        ) {
            source_graph.add_edge(src, tgt, edge.weight);
        }
    }

    let mut graph = fdg::init_force_graph_uniform::<f32, 2, String, f32>(source_graph, 50.0);

    if iterations > 0 {
        FruchtermanReingold::default().apply_many(&mut graph, iterations);
        Center.apply(&mut graph);
    }

    let nodes: Vec<GraphNode> = papers
        .iter()
        .filter_map(|paper| {
            let pid = paper.id.as_deref()?;
            let idx = id_to_idx.get(pid)?;
            let (_node_data, pos) = graph.node_weight(*idx)?;
            let color = paper_tag_color
                .get(pid)
                .cloned()
                .unwrap_or_else(|| "#6b7280".to_string());

            let label = truncate_title(&paper.title, 30);

            Some(GraphNode {
                id: pid.to_string(),
                label,
                x: pos[0] as f64,
                y: pos[1] as f64,
                size: 6.0,
                color,
                is_read: paper.status.is_read,
                is_favorite: paper.status.is_favorite,
            })
        })
        .collect();

    let links: Vec<GraphEdge> = merged_edges
        .into_iter()
        .map(|e| GraphEdge {
            source: e.source,
            target: e.target,
            rel_type: e.rel_type,
            label: e.label,
            weight: e.weight,
        })
        .collect();

    GraphData { nodes, links }
}

fn truncate_title(title: &str, max: usize) -> String {
    if title.len() <= max {
        return title.to_string();
    }
    // Truncate at char boundary
    let mut end = max - 3;
    while !title.is_char_boundary(end) && end > 0 {
        end -= 1;
    }
    format!("{}...", &title[..end])
}
