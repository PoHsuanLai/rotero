//! Friendly descriptors for Rotero's finite MCP tool set.
//!
//! Incoming ACP titles look like `mcp__rotero__get_paper`. The registry
//! matches on the bare function name and never shows hashes, ids, or the
//! `mcp__` prefix. Unknown tools fall through to the generic chip.

use crate::app::chat_papers::{PaperSnippet, papers_from_tool_output};
use rotero_models::{Collection, Paper, Tag};

/// How a known Rotero tool's result should render.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultKind {
    Papers,
    Notes,
    Annotations,
    Names,
    Confirmation,
    Graph,
    Relationships,
    PdfUrls,
    ExtractedText,
}

#[derive(Debug, Clone, Copy)]
pub struct ToolMeta {
    pub label: &'static str,
    pub icon: &'static str,
    pub result: ResultKind,
}

/// Owned chip copy for a known Rotero tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoteroChip {
    pub icon: &'static str,
    pub label: String,
    pub summary: String,
    pub result: ResultKind,
}

pub struct ToolLookups<'a> {
    pub papers: &'a [Paper],
    pub collections: &'a [Collection],
    pub tags: &'a [Tag],
}

impl<'a> ToolLookups<'a> {
    fn paper_title(&self, id: &str) -> Option<&'a str> {
        self.papers
            .iter()
            .find(|p| p.id.as_deref() == Some(id))
            .map(|p| p.title.as_str())
            .filter(|t| !t.is_empty())
    }

    fn collection_name(&self, id: &str) -> Option<&'a str> {
        self.collections
            .iter()
            .find(|c| c.id.as_deref() == Some(id))
            .map(|c| c.name.as_str())
            .filter(|t| !t.is_empty())
    }

    fn tag_name(&self, id: &str) -> Option<&'a str> {
        self.tags
            .iter()
            .find(|t| t.id.as_deref() == Some(id))
            .map(|t| t.name.as_str())
            .filter(|t| !t.is_empty())
    }
}

/// Strip `mcp__rotero__` (and similar) so the registry keys on the fn name.
pub fn bare_tool_name(title: &str) -> &str {
    title
        .rsplit("__")
        .next()
        .unwrap_or(title)
        .rsplit(['/', ':'])
        .next()
        .unwrap_or(title)
        .trim()
}

pub fn descriptor(name: &str) -> Option<ToolMeta> {
    Some(match name {
        "search_papers" => ToolMeta {
            label: "Searched papers",
            icon: "bi bi-search",
            result: ResultKind::Papers,
        },
        "search_online" => ToolMeta {
            label: "Searched online",
            icon: "bi bi-globe",
            result: ResultKind::Papers,
        },
        "find_pdf" => ToolMeta {
            label: "Found PDF",
            icon: "bi bi-file-earmark-pdf",
            result: ResultKind::PdfUrls,
        },
        "get_paper" => ToolMeta {
            label: "Opened paper",
            icon: "bi bi-file-text",
            result: ResultKind::Papers,
        },
        "list_papers" => ToolMeta {
            label: "Listed papers",
            icon: "bi bi-list-ul",
            result: ResultKind::Papers,
        },
        "get_paper_annotations" => ToolMeta {
            label: "Read annotations",
            icon: "bi bi-highlighter",
            result: ResultKind::Annotations,
        },
        "get_paper_notes" => ToolMeta {
            label: "Read notes",
            icon: "bi bi-journal-text",
            result: ResultKind::Notes,
        },
        "list_collections" => ToolMeta {
            label: "Listed collections",
            icon: "bi bi-folder",
            result: ResultKind::Names,
        },
        "list_tags" => ToolMeta {
            label: "Listed tags",
            icon: "bi bi-tags",
            result: ResultKind::Names,
        },
        "get_papers_in_collection" => ToolMeta {
            label: "Listed collection",
            icon: "bi bi-folder2-open",
            result: ResultKind::Papers,
        },
        "get_papers_by_tag" => ToolMeta {
            label: "Listed tagged papers",
            icon: "bi bi-tag",
            result: ResultKind::Papers,
        },
        "extract_pdf_text" => ToolMeta {
            label: "Read PDF",
            icon: "bi bi-file-text",
            result: ResultKind::ExtractedText,
        },
        "add_note" => ToolMeta {
            label: "Added note",
            icon: "bi bi-journal-plus",
            result: ResultKind::Confirmation,
        },
        "update_note" => ToolMeta {
            label: "Updated note",
            icon: "bi bi-pencil",
            result: ResultKind::Confirmation,
        },
        "add_tag_to_paper" => ToolMeta {
            label: "Tagged paper",
            icon: "bi bi-tag",
            result: ResultKind::Confirmation,
        },
        "set_paper_read" => ToolMeta {
            label: "Marked as read",
            icon: "bi bi-check2-circle",
            result: ResultKind::Confirmation,
        },
        "set_paper_favorite" => ToolMeta {
            label: "Favorited",
            icon: "bi bi-star",
            result: ResultKind::Confirmation,
        },
        "add_paper" => ToolMeta {
            label: "Added paper",
            icon: "bi bi-plus-circle",
            result: ResultKind::Confirmation,
        },
        "update_paper" => ToolMeta {
            label: "Updated paper",
            icon: "bi bi-pencil",
            result: ResultKind::Confirmation,
        },
        "delete_paper" => ToolMeta {
            label: "Deleted paper",
            icon: "bi bi-trash",
            result: ResultKind::Confirmation,
        },
        "remove_tag_from_paper" => ToolMeta {
            label: "Removed tag",
            icon: "bi bi-tag",
            result: ResultKind::Confirmation,
        },
        "create_collection" => ToolMeta {
            label: "Created collection",
            icon: "bi bi-folder-plus",
            result: ResultKind::Confirmation,
        },
        "add_paper_to_collection" => ToolMeta {
            label: "Added to collection",
            icon: "bi bi-folder-plus",
            result: ResultKind::Confirmation,
        },
        "remove_paper_from_collection" => ToolMeta {
            label: "Removed from collection",
            icon: "bi bi-folder-minus",
            result: ResultKind::Confirmation,
        },
        "delete_collection" => ToolMeta {
            label: "Deleted collection",
            icon: "bi bi-trash",
            result: ResultKind::Confirmation,
        },
        "rename_collection" => ToolMeta {
            label: "Renamed collection",
            icon: "bi bi-pencil",
            result: ResultKind::Confirmation,
        },
        "rename_tag" => ToolMeta {
            label: "Renamed tag",
            icon: "bi bi-pencil",
            result: ResultKind::Confirmation,
        },
        "delete_tag" => ToolMeta {
            label: "Deleted tag",
            icon: "bi bi-trash",
            result: ResultKind::Confirmation,
        },
        "delete_note" => ToolMeta {
            label: "Deleted note",
            icon: "bi bi-trash",
            result: ResultKind::Confirmation,
        },
        "download_pdf" => ToolMeta {
            label: "Downloaded PDF",
            icon: "bi bi-download",
            result: ResultKind::Confirmation,
        },
        "get_paper_relationships" => ToolMeta {
            label: "Found related papers",
            icon: "bi bi-diagram-3",
            result: ResultKind::Relationships,
        },
        "get_library_graph" => ToolMeta {
            label: "Built citation graph",
            icon: "bi bi-share",
            result: ResultKind::Graph,
        },
        _ => return None,
    })
}

/// Chip / responded-permission label. Never contains `mcp__`.
pub fn humanize_tool_title(title: &str) -> String {
    let name = bare_tool_name(title);
    let label = descriptor(name)
        .map(|m| m.label.to_string())
        .unwrap_or_else(|| {
            if name.is_empty() {
                "a tool".into()
            } else {
                name.to_string()
            }
        });
    if label.contains("mcp__") {
        "a tool".into()
    } else {
        label
    }
}

/// Present-tense action for the allow prompt, e.g. `open a paper`.
pub fn permission_action(name: &str) -> Option<&'static str> {
    Some(match name {
        "search_papers" => "search your papers",
        "search_online" => "search online for papers",
        "find_pdf" => "find a PDF",
        "get_paper" => "open a paper",
        "list_papers" => "list your papers",
        "get_paper_annotations" => "read annotations",
        "get_paper_notes" => "read notes",
        "list_collections" => "list collections",
        "list_tags" => "list tags",
        "get_papers_in_collection" => "list papers in a collection",
        "get_papers_by_tag" => "list tagged papers",
        "extract_pdf_text" => "read a PDF",
        "add_note" => "add a note",
        "update_note" => "update a note",
        "add_tag_to_paper" => "tag a paper",
        "set_paper_read" => "mark a paper as read",
        "set_paper_favorite" => "favorite a paper",
        "add_paper" => "add a paper",
        "update_paper" => "update a paper",
        "delete_paper" => "delete a paper",
        "remove_tag_from_paper" => "remove a tag",
        "create_collection" => "create a collection",
        "add_paper_to_collection" => "add a paper to a collection",
        "remove_paper_from_collection" => "remove a paper from a collection",
        "delete_collection" => "delete a collection",
        "rename_collection" => "rename a collection",
        "rename_tag" => "rename a tag",
        "delete_tag" => "delete a tag",
        "delete_note" => "delete a note",
        "download_pdf" => "download a PDF",
        "get_paper_relationships" => "find related papers",
        "get_library_graph" => "build the citation graph",
        _ => return None,
    })
}

/// Allow-prompt copy. Known Rotero tools read as `Allow Rotero to …?`.
pub fn permission_prompt(title: &str) -> String {
    let name = bare_tool_name(title);
    if let Some(action) = permission_action(name) {
        format!("Allow Rotero to {action}?")
    } else {
        format!("Allow {}?", humanize_tool_title(title))
    }
}

pub fn describe_tool(
    title: &str,
    raw_input: &Option<serde_json::Value>,
    output: Option<&str>,
    lookups: &ToolLookups<'_>,
) -> Option<RoteroChip> {
    let name = bare_tool_name(title);
    let meta = descriptor(name)?;
    let input = raw_input.as_ref();
    let label = dynamic_label(name, input, meta.label);
    let summary = tool_summary(name, input, output, lookups);
    Some(RoteroChip {
        icon: meta.icon,
        label,
        summary,
        result: meta.result,
    })
}

fn dynamic_label(name: &str, input: Option<&serde_json::Value>, fallback: &'static str) -> String {
    match name {
        "set_paper_read" => {
            if input
                .and_then(|v| v.get("is_read"))
                .and_then(|v| v.as_bool())
                == Some(false)
            {
                "Marked as unread".into()
            } else {
                fallback.into()
            }
        }
        "set_paper_favorite" => {
            if input
                .and_then(|v| v.get("is_favorite"))
                .and_then(|v| v.as_bool())
                == Some(false)
            {
                "Removed favorite".into()
            } else {
                fallback.into()
            }
        }
        _ => fallback.into(),
    }
}

fn tool_summary(
    name: &str,
    input: Option<&serde_json::Value>,
    output: Option<&str>,
    lookups: &ToolLookups<'_>,
) -> String {
    match name {
        "search_papers" | "search_online" => input_str(input, &["query", "q"])
            .map(|q| truncate(&q, 60))
            .unwrap_or_default(),
        "find_pdf" => input_str(input, &["title"])
            .filter(|s| !looks_like_id(s))
            .map(|t| quote(&t))
            .or_else(|| input_str(input, &["doi"]).map(|d| truncate(&d, 40)))
            .unwrap_or_default(),
        "get_paper"
        | "get_paper_annotations"
        | "get_paper_notes"
        | "extract_pdf_text"
        | "set_paper_read"
        | "set_paper_favorite"
        | "update_paper"
        | "delete_paper"
        | "download_pdf"
        | "get_paper_relationships" => named_paper(input, output, lookups),
        "list_papers" => output.and_then(paper_count_hint).unwrap_or_default(),
        "get_papers_in_collection" => named_collection(input, lookups),
        "get_papers_by_tag" => named_tag(input, lookups),
        "add_note" => {
            let paper = named_paper(input, output, lookups);
            match input_str(input, &["title"]) {
                Some(note) if !paper.is_empty() => format!("{paper} — {note}"),
                Some(note) => note,
                None => paper,
            }
        }
        "update_note" | "delete_note" => input_str(input, &["title"]).unwrap_or_default(),
        "add_tag_to_paper" => {
            let papers = named_papers(input, output, lookups);
            let tags = input_string_list(input, "tag_names");
            match (papers.is_empty(), tags.is_empty()) {
                (false, false) => format!("{papers} as {tags}"),
                (false, true) => papers,
                (true, false) => tags,
                (true, true) => String::new(),
            }
        }
        "remove_tag_from_paper" => {
            let papers = named_papers(input, output, lookups);
            let tags = named_tags(input, lookups);
            match (papers.is_empty(), tags.is_empty()) {
                (false, false) => format!("{tags} from {papers}"),
                (false, true) => papers,
                (true, false) => tags,
                (true, true) => String::new(),
            }
        }
        "add_paper" => input_str(input, &["title"])
            .filter(|s| !looks_like_id(s))
            .map(|t| quote(&t))
            .unwrap_or_else(|| named_paper(input, output, lookups)),
        "create_collection" | "rename_collection" => input_str(input, &["name"])
            .filter(|s| !looks_like_id(s))
            .map(|n| quote(&n))
            .unwrap_or_else(|| named_collection(input, lookups)),
        "add_paper_to_collection" | "remove_paper_from_collection" => {
            let papers = named_papers(input, output, lookups);
            let colls = named_collections(input, lookups);
            match (papers.is_empty(), colls.is_empty()) {
                (false, false) => format!("{papers} → {colls}"),
                (false, true) => papers,
                (true, false) => colls,
                (true, true) => String::new(),
            }
        }
        "delete_collection" => named_collection(input, lookups),
        "rename_tag" => input_str(input, &["name"])
            .filter(|s| !looks_like_id(s))
            .map(|n| quote(&n))
            .unwrap_or_else(|| named_tag(input, lookups)),
        "delete_tag" => named_tag(input, lookups),
        "get_library_graph" => graph_summary(output).unwrap_or_default(),
        "list_collections" | "list_tags" => String::new(),
        _ => String::new(),
    }
}

/// One-line confirmation for mutation tools. Never includes ids or `success`.
pub fn confirmation_line(chip: &RoteroChip) -> String {
    if chip.summary.is_empty() {
        chip.label.clone()
    } else {
        format!("{} {}", chip.label, chip.summary)
    }
}

fn named_paper(
    input: Option<&serde_json::Value>,
    output: Option<&str>,
    lookups: &ToolLookups<'_>,
) -> String {
    if let Some(title) = input_str(input, &["title"]).filter(|s| !looks_like_id(s)) {
        return quote(&title);
    }
    let Some(id) = first_paper_id(input) else {
        if let Some(title) = title_from_output(output) {
            return quote(&title);
        }
        return String::new();
    };
    quote_or(resolve_paper_title(&id, output, lookups), "a paper")
}

fn named_papers(
    input: Option<&serde_json::Value>,
    output: Option<&str>,
    lookups: &ToolLookups<'_>,
) -> String {
    let ids = paper_ids(input);
    if ids.is_empty() {
        return named_paper(input, output, lookups);
    }
    let titles: Vec<String> = ids
        .iter()
        .filter_map(|id| resolve_paper_title(id, output, lookups))
        .collect();
    join_quoted(&titles, ids.len(), "paper", "papers")
}

fn named_collection(input: Option<&serde_json::Value>, lookups: &ToolLookups<'_>) -> String {
    if let Some(name) = input_str(input, &["name"]).filter(|s| !looks_like_id(s)) {
        return quote(&name);
    }
    let Some(id) = input_str(input, &["collection_id"]) else {
        return String::new();
    };
    quote_or(
        lookups.collection_name(&id).map(str::to_string),
        "a collection",
    )
}

fn named_collections(input: Option<&serde_json::Value>, lookups: &ToolLookups<'_>) -> String {
    let ids = string_list(input, "collection_ids");
    if ids.is_empty() {
        return named_collection(input, lookups);
    }
    let names: Vec<String> = ids
        .iter()
        .filter_map(|id| lookups.collection_name(id).map(str::to_string))
        .collect();
    join_quoted(&names, ids.len(), "collection", "collections")
}

fn named_tag(input: Option<&serde_json::Value>, lookups: &ToolLookups<'_>) -> String {
    if let Some(name) = input_str(input, &["name"]).filter(|s| !looks_like_id(s)) {
        return quote(&name);
    }
    let Some(id) = input_str(input, &["tag_id"]) else {
        return String::new();
    };
    quote_or(lookups.tag_name(&id).map(str::to_string), "a tag")
}

fn named_tags(input: Option<&serde_json::Value>, lookups: &ToolLookups<'_>) -> String {
    let names = input_string_list(input, "tag_names");
    if !names.is_empty() {
        return names;
    }
    let ids = string_list(input, "tag_ids");
    if ids.is_empty() {
        return named_tag(input, lookups);
    }
    let names: Vec<String> = ids
        .iter()
        .filter_map(|id| lookups.tag_name(id).map(str::to_string))
        .collect();
    join_quoted(&names, ids.len(), "tag", "tags")
}

fn resolve_paper_title(
    id: &str,
    output: Option<&str>,
    lookups: &ToolLookups<'_>,
) -> Option<String> {
    if let Some(title) = lookups.paper_title(id) {
        return Some(title.to_string());
    }
    let papers = output.map(papers_from_tool_output).unwrap_or_default();
    papers
        .into_iter()
        .find(|p| p.id.as_deref() == Some(id))
        .map(|p| p.title)
        .filter(|t| !t.is_empty())
}

fn title_from_output(output: Option<&str>) -> Option<String> {
    let mut papers = output.map(papers_from_tool_output).unwrap_or_default();
    if papers.len() == 1 {
        let title = papers.remove(0).title;
        if title.is_empty() { None } else { Some(title) }
    } else {
        None
    }
}

fn quote_or(title: Option<String>, fallback: &str) -> String {
    match title {
        Some(t) => quote(&t),
        None => fallback.into(),
    }
}

fn join_quoted(names: &[String], total: usize, singular: &str, plural: &str) -> String {
    match (total, names) {
        (0, _) => String::new(),
        (1, [n]) => quote(n),
        (1, []) => format!("a {singular}"),
        (2, [a, b]) => format!("{} and {}", quote(a), quote(b)),
        (_, names) if names.len() == total && total <= 2 => names
            .iter()
            .map(|n| quote(n))
            .collect::<Vec<_>>()
            .join(" and "),
        (n, _) => format!("{n} {plural}"),
    }
}

fn quote(s: &str) -> String {
    format!("\"{}\"", truncate(s, 60))
}

fn first_paper_id(input: Option<&serde_json::Value>) -> Option<String> {
    paper_ids(input).into_iter().next()
}

fn paper_ids(input: Option<&serde_json::Value>) -> Vec<String> {
    let mut ids = string_list(input, "paper_ids");
    if ids.is_empty()
        && let Some(id) = input_str(input, &["paper_id"])
    {
        ids.push(id);
    }
    ids
}

fn input_str(input: Option<&serde_json::Value>, keys: &[&str]) -> Option<String> {
    let map = input.and_then(|v| v.as_object())?;
    for key in keys {
        if let Some(s) = map
            .get(*key)
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
        {
            return Some(s.to_string());
        }
    }
    None
}

fn string_list(input: Option<&serde_json::Value>, key: &str) -> Vec<String> {
    input
        .and_then(|v| v.get(key))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn input_string_list(input: Option<&serde_json::Value>, key: &str) -> String {
    let names: Vec<String> = string_list(input, key)
        .into_iter()
        .filter(|s| !looks_like_id(s))
        .collect();
    match names.as_slice() {
        [] => String::new(),
        [one] => one.clone(),
        [a, b] => format!("{a} and {b}"),
        rest => rest.join(", "),
    }
}

pub fn looks_like_id(s: &str) -> bool {
    let s = s.trim();
    if s.len() < 16 {
        return false;
    }
    if !s.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
        return false;
    }
    s.chars().filter(|c| c.is_ascii_hexdigit()).count() >= 16
}

fn truncate(s: &str, max: usize) -> String {
    let mut chars = s.chars();
    let head: String = chars.by_ref().take(max).collect();
    if chars.next().is_some() {
        format!("{head}…")
    } else {
        head
    }
}

fn paper_count_hint(output: &str) -> Option<String> {
    let value = serde_json::from_str::<serde_json::Value>(output).ok()?;
    let total = value.get("total").and_then(|v| v.as_u64());
    let n = papers_from_tool_output(output).len();
    match (total, n) {
        (Some(t), _) => Some(format!("{t} in library")),
        (None, 0) => None,
        (None, 1) => Some("1 paper".into()),
        (None, n) => Some(format!("{n} papers")),
    }
}

pub fn graph_summary(output: Option<&str>) -> Option<String> {
    let value = serde_json::from_str::<serde_json::Value>(output?).ok()?;
    let nodes = value
        .get("nodes")
        .and_then(|v| v.as_array())
        .map(Vec::len)?;
    let edges = value
        .get("edges")
        .and_then(|v| v.as_array())
        .map(Vec::len)?;
    Some(format!(
        "{nodes} {} · {edges} {}",
        if nodes == 1 { "paper" } else { "papers" },
        if edges == 1 {
            "connection"
        } else {
            "connections"
        }
    ))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteSnippet {
    pub title: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnnotationSnippet {
    pub page: i32,
    pub kind: String,
    pub content: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelSnippet {
    pub title: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedText {
    pub text: String,
    pub page_start: u32,
    pub page_end: u32,
    pub total_pages: u32,
}

pub fn notes_from_output(output: &str) -> Vec<NoteSnippet> {
    let mut notes = Vec::new();
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(output) {
        collect_notes(&value, &mut notes);
    }
    notes
}

fn collect_notes(value: &serde_json::Value, notes: &mut Vec<NoteSnippet>) {
    match value {
        serde_json::Value::Object(map) => {
            let is_note = map.contains_key("body")
                && map.contains_key("title")
                && !map.contains_key("creators");
            if is_note {
                let title = map
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let body = map
                    .get("body")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if !title.is_empty() || !body.is_empty() {
                    notes.push(NoteSnippet { title, body });
                }
            } else {
                for nested in map.values() {
                    collect_notes(nested, notes);
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect_notes(item, notes);
            }
        }
        _ => {}
    }
}

pub fn annotations_from_output(output: &str) -> Vec<AnnotationSnippet> {
    let mut anns = Vec::new();
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(output) {
        collect_annotations(&value, &mut anns);
    }
    anns
}

fn collect_annotations(value: &serde_json::Value, anns: &mut Vec<AnnotationSnippet>) {
    match value {
        serde_json::Value::Object(map) => {
            let is_ann = map.contains_key("ann_type")
                || (map.contains_key("page") && map.contains_key("geometry"));
            if is_ann {
                let page = map.get("page").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                let kind = map
                    .get("ann_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Annotation")
                    .to_string();
                let content = map
                    .get("content")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(str::to_string);
                anns.push(AnnotationSnippet {
                    page,
                    kind,
                    content,
                });
            } else {
                for nested in map.values() {
                    collect_annotations(nested, anns);
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect_annotations(item, anns);
            }
        }
        _ => {}
    }
}

pub fn names_from_output(output: &str) -> Vec<String> {
    let mut names = Vec::new();
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(output) {
        collect_names(&value, &mut names);
    }
    names
}

fn collect_names(value: &serde_json::Value, names: &mut Vec<String>) {
    match value {
        serde_json::Value::Object(map) => {
            if let Some(name) = map.get("name").and_then(|v| v.as_str())
                && !name.is_empty()
                && !map.contains_key("creators")
                && !map.contains_key("title")
            {
                if !names.iter().any(|seen| seen == name) {
                    names.push(name.to_string());
                }
                return;
            }
            for nested in map.values() {
                collect_names(nested, names);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect_names(item, names);
            }
        }
        _ => {}
    }
}

pub fn relationships_from_output(output: &str) -> Vec<RelSnippet> {
    let mut rels = Vec::new();
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(output) {
        collect_rels(&value, &mut rels);
    }
    rels
}

fn collect_rels(value: &serde_json::Value, rels: &mut Vec<RelSnippet>) {
    match value {
        serde_json::Value::Object(map) => {
            if let Some(title) = map
                .get("related_paper_title")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
            {
                let label = map
                    .get("label")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .or_else(|| map.get("relationship_type").and_then(|v| v.as_str()))
                    .unwrap_or("")
                    .to_string();
                rels.push(RelSnippet {
                    title: title.to_string(),
                    label,
                });
            } else {
                for nested in map.values() {
                    collect_rels(nested, rels);
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect_rels(item, rels);
            }
        }
        _ => {}
    }
}

pub fn urls_from_output(output: &str) -> Vec<String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(output) else {
        return Vec::new();
    };
    match value {
        serde_json::Value::Array(items) => items
            .into_iter()
            .filter_map(|v| match v {
                serde_json::Value::String(s) if !s.is_empty() => Some(s),
                serde_json::Value::Object(map) => map
                    .get("url")
                    .or_else(|| map.get("pdf_url"))
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(str::to_string),
                _ => None,
            })
            .collect(),
        serde_json::Value::String(s) if s.starts_with("http") => vec![s],
        _ => Vec::new(),
    }
}

pub fn extracted_text_from_output(output: &str) -> Option<ExtractedText> {
    let value = serde_json::from_str::<serde_json::Value>(output).ok()?;
    let text = value.get("text").and_then(|v| v.as_str())?.to_string();
    Some(ExtractedText {
        text,
        page_start: value
            .get("page_start")
            .and_then(|v| v.as_u64())
            .unwrap_or(1) as u32,
        page_end: value.get("page_end").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        total_pages: value
            .get("total_pages")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32,
    })
}

pub fn papers_for_result(output: Option<&str>) -> Vec<PaperSnippet> {
    output.map(papers_from_tool_output).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paper(id: &str, title: &str) -> Paper {
        Paper {
            id: Some(id.to_string()),
            title: title.to_string(),
            ..Default::default()
        }
    }

    fn lookups<'a>(
        papers: &'a [Paper],
        collections: &'a [Collection],
        tags: &'a [Tag],
    ) -> ToolLookups<'a> {
        ToolLookups {
            papers,
            collections,
            tags,
        }
    }

    fn empty_lookups<'a>() -> ToolLookups<'a> {
        ToolLookups {
            papers: &[],
            collections: &[],
            tags: &[],
        }
    }

    const ROTERO_TOOL_NAMES: &[&str] = &[
        "search_papers",
        "search_online",
        "find_pdf",
        "get_paper",
        "list_papers",
        "get_paper_annotations",
        "get_paper_notes",
        "list_collections",
        "list_tags",
        "get_papers_in_collection",
        "get_papers_by_tag",
        "extract_pdf_text",
        "add_note",
        "update_note",
        "add_tag_to_paper",
        "set_paper_read",
        "set_paper_favorite",
        "add_paper",
        "update_paper",
        "delete_paper",
        "remove_tag_from_paper",
        "create_collection",
        "add_paper_to_collection",
        "remove_paper_from_collection",
        "delete_collection",
        "rename_collection",
        "rename_tag",
        "delete_tag",
        "delete_note",
        "download_pdf",
        "get_paper_relationships",
        "get_library_graph",
    ];

    const PAPER_ID: &str = "11111111-1111-7111-8111-111111111111";

    #[test]
    fn strips_mcp_prefix() {
        assert_eq!(bare_tool_name("mcp__rotero__get_paper"), "get_paper");
        assert_eq!(bare_tool_name("get_paper"), "get_paper");
        assert_eq!(bare_tool_name("rotero__search_papers"), "search_papers");
    }

    #[test]
    fn every_known_tool_has_a_descriptor() {
        for name in ROTERO_TOOL_NAMES {
            assert!(descriptor(name).is_some(), "missing descriptor for {name}");
        }
        assert_eq!(ROTERO_TOOL_NAMES.len(), 32);
        assert!(descriptor("web_search").is_none());
        assert!(descriptor("mcp__rotero__nope").is_none());
        assert!(descriptor(bare_tool_name("mcp__rotero__get_paper")).is_some());
    }

    #[test]
    fn permission_titles_never_show_mcp_prefix() {
        for name in ROTERO_TOOL_NAMES {
            let raw = format!("mcp__rotero__{name}");
            let chip = humanize_tool_title(&raw);
            let prompt = permission_prompt(&raw);
            assert!(
                !chip.contains("mcp__"),
                "chip leaked mcp__ for {raw}: {chip}"
            );
            assert!(
                !prompt.contains("mcp__"),
                "prompt leaked mcp__ for {raw}: {prompt}"
            );
            assert_eq!(chip, descriptor(name).unwrap().label);
            assert!(
                permission_action(name).is_some(),
                "missing permission action for {name}"
            );
            assert_eq!(
                prompt,
                format!("Allow Rotero to {}?", permission_action(name).unwrap())
            );
        }
        assert_eq!(
            humanize_tool_title("mcp__rotero__get_paper"),
            "Opened paper"
        );
        assert_eq!(
            permission_prompt("mcp__rotero__get_paper"),
            "Allow Rotero to open a paper?"
        );
        assert_eq!(humanize_tool_title("mcp__other__web_search"), "web_search");
        assert_eq!(
            permission_prompt("mcp__other__web_search"),
            "Allow web_search?"
        );
        assert!(!permission_prompt("mcp__other__web_search").contains("mcp__"));
    }

    #[test]
    fn get_paper_uses_library_title_not_id() {
        let papers = vec![paper(PAPER_ID, "Attention Is All You Need")];
        let chip = describe_tool(
            "mcp__rotero__get_paper",
            &Some(serde_json::json!({ "paper_id": PAPER_ID })),
            None,
            &lookups(&papers, &[], &[]),
        )
        .unwrap();
        assert_eq!(chip.label, "Opened paper");
        assert_eq!(chip.summary, "\"Attention Is All You Need\"");
        assert!(!chip.summary.contains(PAPER_ID));
        assert_eq!(chip.result, ResultKind::Papers);
        assert!(!chip.icon.is_empty());
    }

    #[test]
    fn unresolved_paper_id_falls_back_without_the_hash() {
        let chip = describe_tool(
            "get_paper",
            &Some(serde_json::json!({ "paper_id": PAPER_ID })),
            None,
            &empty_lookups(),
        )
        .unwrap();
        assert_eq!(chip.summary, "a paper");
        assert!(!chip.summary.contains(PAPER_ID));
        assert!(!looks_like_id(&chip.summary));
    }

    #[test]
    fn get_paper_title_from_tool_output_when_library_is_empty() {
        let output = serde_json::to_string(&paper(PAPER_ID, "Attention Is All You Need")).unwrap();
        let chip = describe_tool(
            "get_paper",
            &Some(serde_json::json!({ "paper_id": PAPER_ID })),
            Some(&output),
            &empty_lookups(),
        )
        .unwrap();
        assert_eq!(chip.summary, "\"Attention Is All You Need\"");
    }

    #[test]
    fn search_summary_is_the_query() {
        let chip = describe_tool(
            "search_papers",
            &Some(serde_json::json!({ "query": "attention transformers" })),
            None,
            &empty_lookups(),
        )
        .unwrap();
        assert_eq!(chip.label, "Searched papers");
        assert_eq!(chip.summary, "attention transformers");
        assert_eq!(chip.result, ResultKind::Papers);
    }

    #[test]
    fn add_tag_summary_uses_title_and_tag_name() {
        let papers = vec![paper(PAPER_ID, "Attention Is All You Need")];
        let chip = describe_tool(
            "add_tag_to_paper",
            &Some(serde_json::json!({
                "paper_ids": [PAPER_ID],
                "tag_names": ["transformers"]
            })),
            None,
            &lookups(&papers, &[], &[]),
        )
        .unwrap();
        assert_eq!(
            chip.summary,
            "\"Attention Is All You Need\" as transformers"
        );
        assert!(!chip.summary.contains(PAPER_ID));
        assert_eq!(chip.result, ResultKind::Confirmation);
        assert_eq!(
            confirmation_line(&chip),
            "Tagged paper \"Attention Is All You Need\" as transformers"
        );
    }

    #[test]
    fn set_paper_read_label_tracks_the_flag() {
        let papers = vec![paper(PAPER_ID, "Attention Is All You Need")];
        let read = describe_tool(
            "set_paper_read",
            &Some(serde_json::json!({ "paper_id": PAPER_ID, "is_read": true })),
            None,
            &lookups(&papers, &[], &[]),
        )
        .unwrap();
        assert_eq!(read.label, "Marked as read");
        assert_eq!(read.summary, "\"Attention Is All You Need\"");
        assert_eq!(
            confirmation_line(&read),
            "Marked as read \"Attention Is All You Need\""
        );

        let unread = describe_tool(
            "set_paper_read",
            &Some(serde_json::json!({ "paper_id": PAPER_ID, "is_read": false })),
            None,
            &lookups(&papers, &[], &[]),
        )
        .unwrap();
        assert_eq!(unread.label, "Marked as unread");
    }

    #[test]
    fn unknown_tools_have_no_descriptor() {
        assert!(describe_tool("bash", &None, None, &empty_lookups()).is_none());
        assert!(describe_tool("mcp__other__search", &None, None, &empty_lookups()).is_none());
    }

    #[test]
    fn graph_summary_counts_nodes_and_edges() {
        let json = r#"{"nodes":[{"title":"A"},{"title":"B"}],"edges":[{},{},{}]}"#;
        assert_eq!(
            graph_summary(Some(json)).as_deref(),
            Some("2 papers · 3 connections")
        );
        let chip = describe_tool("get_library_graph", &None, Some(json), &empty_lookups()).unwrap();
        assert_eq!(chip.summary, "2 papers · 3 connections");
        assert_eq!(chip.result, ResultKind::Graph);
    }

    #[test]
    fn notes_and_annotations_hide_ids() {
        let notes = notes_from_output(
            r#"[{"id":"aaaaaaaa-aaaa-7aaa-8aaa-aaaaaaaaaaaa","paper_id":"11111111-1111-7111-8111-111111111111","title":"Idea","body":"Compare to BERT"}]"#,
        );
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].title, "Idea");
        assert_eq!(notes[0].body, "Compare to BERT");

        let anns = annotations_from_output(
            r#"[{"id":"bbbbbbbb-bbbb-7bbb-8bbb-bbbbbbbbbbbb","page":3,"ann_type":"Highlight","content":"attention","geometry":{"x":1}}]"#,
        );
        assert_eq!(anns.len(), 1);
        assert_eq!(anns[0].page, 3);
        assert_eq!(anns[0].kind, "Highlight");
        assert_eq!(anns[0].content.as_deref(), Some("attention"));
    }

    #[test]
    fn looks_like_id_catches_uuids_only() {
        assert!(looks_like_id(PAPER_ID));
        assert!(!looks_like_id("attention"));
        assert!(!looks_like_id("10.1234/abc"));
    }
}
