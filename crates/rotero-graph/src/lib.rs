//! Citation network graph for the paper library. Computes edges from shared
//! tags, authors, collections, and journals, then runs a force-directed layout
//! via petgraph and fdg.

/// Data types for graph nodes, edges, and filters.
pub mod data;
/// Edge computation: pairwise relationships between papers.
pub mod edges;

use std::collections::HashMap;

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
        if !paper_tag_color.contains_key(paper_id.as_str())
            && let Some(&color) = tag_colors.get(tag_id.as_str())
        {
            paper_tag_color.insert(paper_id.as_str(), color.to_string());
        }
    }

    // Group papers by their primary tag color for cluster-aware initial placement.
    // Each group is placed in a sector of a circle so connected nodes start nearby.
    let mut color_groups: HashMap<String, Vec<&str>> = HashMap::new();
    for paper in papers {
        if let Some(ref pid) = paper.id {
            let color = paper_tag_color
                .get(pid.as_str())
                .cloned()
                .unwrap_or_else(|| "#6b7280".to_string());
            color_groups.entry(color).or_default().push(pid.as_str());
        }
    }

    let num_groups = color_groups.len().max(1);
    let group_radius = 80.0_f32 * (num_groups as f32).sqrt();
    let mut paper_init_pos: HashMap<&str, [f32; 2]> = HashMap::new();

    for (gi, (_color, pids)) in color_groups.iter().enumerate() {
        let angle = 2.0 * std::f32::consts::PI * (gi as f32) / (num_groups as f32);
        let center_x = group_radius * angle.cos();
        let center_y = group_radius * angle.sin();
        let inner_radius = 20.0_f32 * (pids.len() as f32).sqrt();
        for (ni, pid) in pids.iter().enumerate() {
            let a2 = 2.0 * std::f32::consts::PI * (ni as f32) / (pids.len() as f32);
            let r = inner_radius * ((ni as f32 + 1.0) / pids.len() as f32).sqrt();
            paper_init_pos.insert(pid, [center_x + r * a2.cos(), center_y + r * a2.sin()]);
        }
    }

    // Build node position/velocity arrays for our own spring simulation
    // that matches the JS physics constants exactly.
    let paper_ids: Vec<&str> = papers.iter().filter_map(|p| p.id.as_deref()).collect();
    let n = paper_ids.len();
    let mut pos: Vec<[f64; 2]> = paper_ids
        .iter()
        .map(|pid| {
            paper_init_pos
                .get(pid)
                .map(|p| [p[0] as f64, p[1] as f64])
                .unwrap_or([0.0, 0.0])
        })
        .collect();
    let mut vel: Vec<[f64; 2]> = vec![[0.0; 2]; n];

    let pid_to_sim: HashMap<&str, usize> = paper_ids
        .iter()
        .enumerate()
        .map(|(i, &pid)| (pid, i))
        .collect();

    // Edge list as sim indices
    let sim_edges: Vec<(usize, usize, f64)> = merged_edges
        .iter()
        .filter_map(|e| {
            let a = *pid_to_sim.get(e.source.as_str())?;
            let b = *pid_to_sim.get(e.target.as_str())?;
            Some((a, b, e.weight as f64))
        })
        .collect();

    // Constants matching JS
    const REPULSION: f64 = 300.0;
    const SPRING_K: f64 = 0.005;
    const SPRING_LENGTH: f64 = 120.0;
    const CENTER_GRAVITY: f64 = 0.002;
    const DAMPING: f64 = 0.4;
    const MAX_VELOCITY: f64 = 0.4;

    for _ in 0..iterations {
        // Repulsion O(n²)
        for i in 0..n {
            for j in (i + 1)..n {
                let dx = pos[i][0] - pos[j][0];
                let dy = pos[i][1] - pos[j][1];
                let dist = (dx * dx + dy * dy).sqrt().max(1.0);
                let force = REPULSION / (dist * dist);
                let fx = (dx / dist) * force;
                let fy = (dy / dist) * force;
                vel[i][0] += fx;
                vel[i][1] += fy;
                vel[j][0] -= fx;
                vel[j][1] -= fy;
            }
        }

        // Spring forces along edges
        for &(a, b, weight) in &sim_edges {
            let dx = pos[b][0] - pos[a][0];
            let dy = pos[b][1] - pos[a][1];
            let dist = (dx * dx + dy * dy).sqrt().max(1.0);
            let displacement = dist - SPRING_LENGTH;
            let force = SPRING_K * displacement * weight;
            let fx = (dx / dist) * force;
            let fy = (dy / dist) * force;
            vel[a][0] += fx;
            vel[a][1] += fy;
            vel[b][0] -= fx;
            vel[b][1] -= fy;
        }

        // Center gravity
        let mut cx = 0.0;
        let mut cy = 0.0;
        for p in &pos {
            cx += p[0];
            cy += p[1];
        }
        cx /= n as f64;
        cy /= n as f64;

        // Damping + velocity cap + integrate
        for i in 0..n {
            vel[i][0] -= (pos[i][0] - cx) * CENTER_GRAVITY;
            vel[i][1] -= (pos[i][1] - cy) * CENTER_GRAVITY;
            vel[i][0] *= DAMPING;
            vel[i][1] *= DAMPING;
            let speed = (vel[i][0] * vel[i][0] + vel[i][1] * vel[i][1]).sqrt();
            if speed > MAX_VELOCITY {
                vel[i][0] = (vel[i][0] / speed) * MAX_VELOCITY;
                vel[i][1] = (vel[i][1] / speed) * MAX_VELOCITY;
            }
            pos[i][0] += vel[i][0];
            pos[i][1] += vel[i][1];
        }
    }

    let nodes: Vec<GraphNode> = papers
        .iter()
        .filter_map(|paper| {
            let pid = paper.id.as_deref()?;
            let si = *pid_to_sim.get(pid)?;
            let color = paper_tag_color
                .get(pid)
                .cloned()
                .unwrap_or_else(|| "#6b7280".to_string());

            let label = truncate_title(&paper.title, 30);

            Some(GraphNode {
                id: pid.to_string(),
                label,
                x: pos[si][0],
                y: pos[si][1],
                size: 3.5,
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
