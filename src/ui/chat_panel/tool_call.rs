use dioxus::prelude::*;

use super::rotero_tools::{
    ResultKind, RoteroChip, ToolLookups, annotations_from_output, confirmation_line, describe_tool,
    extracted_text_from_output, graph_summary, looks_like_id, names_from_output, notes_from_output,
    papers_for_result, relationships_from_output, urls_from_output,
};
use crate::agent::types::{ToolContentBlock, ToolKind, ToolStatus, ToolUse};
use crate::app::chat_papers::PaperSnippet;
use crate::state::app_state::LibraryState;

#[component]
pub(crate) fn ToolCallView(tool: ToolUse) -> Element {
    let mut expanded = use_signal(|| false);
    let mut copied = use_signal(|| false);
    let lib_state = use_context::<Signal<LibraryState>>();

    let (status_icon, status_class, status_label) = match tool.status {
        ToolStatus::Pending => ("bi bi-clock", "chat-tool-call--running", "pending"),
        ToolStatus::InProgress => ("bi bi-clock", "chat-tool-call--running", "running"),
        ToolStatus::Completed => ("bi bi-check2", "chat-tool-call--done", "done"),
        ToolStatus::Failed => ("bi bi-x-lg", "chat-tool-call--failed", "failed"),
    };
    let rotero = {
        let lib = lib_state.read();
        describe_tool(
            &tool.title,
            &tool.raw_input,
            tool.output.as_deref(),
            &ToolLookups {
                papers: &lib.papers,
                collections: &lib.collections,
                tags: &lib.tags,
            },
        )
    };
    let icon_class = rotero
        .as_ref()
        .map(|r| r.icon)
        .unwrap_or_else(|| kind_icon(tool.kind));
    let label_text = rotero
        .as_ref()
        .map(|r| r.label.clone())
        .unwrap_or_else(|| kind_label(tool.kind).to_string());
    let summary = rotero
        .as_ref()
        .map(|r| r.summary.clone())
        .unwrap_or_else(|| input_summary(&tool.title, &tool.raw_input));
    let name_class = if rotero.is_some() {
        "chat-tool-name chat-tool-name--plain"
    } else {
        "chat-tool-name"
    };
    let chevron = if expanded() {
        "bi bi-chevron-down"
    } else {
        "bi bi-chevron-right"
    };
    let open_class = if expanded() {
        "chat-tool-call--open"
    } else {
        ""
    };

    let diffs: Vec<(String, Option<String>, String)> = tool
        .content
        .iter()
        .filter_map(|block| match block {
            ToolContentBlock::Diff {
                path,
                old_text,
                new_text,
            } => Some((path.clone(), old_text.clone(), new_text.clone())),
            _ => None,
        })
        .collect();
    let resources: Vec<(String, String, Option<String>, Option<String>)> = tool
        .content
        .iter()
        .filter_map(|block| match block {
            ToolContentBlock::Resource {
                title,
                uri,
                mime_type,
                text,
            } => Some((title.clone(), uri.clone(), mime_type.clone(), text.clone())),
            _ => None,
        })
        .collect();
    let papers = tool
        .output
        .as_deref()
        .map(crate::app::chat_papers::papers_from_tool_output)
        .unwrap_or_default();
    let json_value = tool
        .output
        .as_deref()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok());
    let rotero_body = rotero.is_some();
    let has_typed_body =
        !diffs.is_empty() || !resources.is_empty() || !papers.is_empty() || rotero_body;
    let output_text = tool.output.clone();
    let copy_text = tool.output.clone().or_else(|| {
        json_value
            .as_ref()
            .map(|v| serde_json::to_string_pretty(v).unwrap_or_else(|_| v.to_string()))
    });
    let failed = tool.status == ToolStatus::Failed;

    rsx! {
        div { class: "chat-tool",
            button {
                class: "chat-tool-call {status_class} {open_class}",
                r#type: "button",
                onclick: move |_| expanded.set(!expanded()),
                i { class: "{icon_class} chat-tool-kind-icon", title: "{label_text}" }
                span { class: "chat-tool-kind", "{label_text}" }
                if !summary.is_empty() {
                    span { class: "{name_class}", "{summary}" }
                }
                span { class: "chat-tool-pill",
                    i { class: "{status_icon}" }
                    "{status_label}"
                }
                i { class: "{chevron} chat-tool-chevron" }
            }
            if expanded() {
                div { class: "chat-tool-body",
                    if !diffs.is_empty() {
                        for (path, old_text, new_text) in diffs.iter() {
                            DiffView {
                                key: "{path}",
                                path: path.clone(),
                                old_text: old_text.clone(),
                                new_text: new_text.clone(),
                            }
                        }
                    }
                    if !resources.is_empty() {
                        for (title, uri, mime, text) in resources.iter() {
                            ResourceCard {
                                key: "{uri}",
                                title: title.clone(),
                                uri: uri.clone(),
                                mime_type: mime.clone(),
                                text: text.clone(),
                            }
                        }
                    }
                    if let Some(chip) = rotero.clone() {
                        RoteroResult {
                            chip,
                            output: tool.output.clone(),
                            papers: papers.clone(),
                            failed,
                        }
                    } else if !papers.is_empty() {
                        div { class: "chat-paper-list",
                            for paper in papers.iter() {
                                ChatPaperCard { key: "{paper.id.as_deref().unwrap_or(paper.title.as_str())}", paper: paper.clone() }
                            }
                        }
                    }
                    if !has_typed_body {
                        if let Some(value) = json_value.clone() {
                            JsonBlock {
                                value,
                                copied: copied(),
                                on_copy: move |()| {
                                    if let Some(text) = copy_text.clone()
                                        && let Ok(mut clip) = arboard::Clipboard::new()
                                    {
                                        let _ = clip.set_text(text);
                                        copied.set(true);
                                        spawn(async move {
                                            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                                            copied.set(false);
                                        });
                                    }
                                },
                            }
                        } else if let Some(text) = output_text.clone() {
                            div { class: "chat-tool-output",
                                pre { "{text}" }
                            }
                        } else if !tool.locations.is_empty() {
                            ul { class: "chat-tool-locations",
                                for loc in tool.locations.iter() {
                                    li {
                                        span { class: "chat-resource-uri", "{loc.path}" }
                                        if let Some(line) = loc.line {
                                            span { class: "chat-tool-line", ":{line}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn RoteroResult(
    chip: RoteroChip,
    output: Option<String>,
    papers: Vec<PaperSnippet>,
    failed: bool,
) -> Element {
    if failed {
        let msg = output
            .as_deref()
            .map(redact_ids)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "Something went wrong.".into());
        return rsx! {
            p { class: "chat-tool-confirm chat-tool-confirm--failed", "{msg}" }
        };
    }

    match chip.result {
        ResultKind::Papers => {
            let cards = if papers.is_empty() {
                papers_for_result(output.as_deref())
            } else {
                papers
            };
            if cards.is_empty() {
                rsx! { p { class: "chat-tool-confirm", "No papers found." } }
            } else {
                rsx! {
                    div { class: "chat-paper-list",
                        for paper in cards.iter() {
                            ChatPaperCard {
                                key: "{paper.id.as_deref().unwrap_or(paper.title.as_str())}",
                                paper: paper.clone(),
                            }
                        }
                    }
                }
            }
        }
        ResultKind::Notes => {
            let notes = output.as_deref().map(notes_from_output).unwrap_or_default();
            if notes.is_empty() {
                rsx! { p { class: "chat-tool-confirm", "No notes." } }
            } else {
                rsx! {
                    div { class: "chat-note-list",
                        for (i, note) in notes.iter().enumerate() {
                            div { key: "{i}", class: "chat-note-card",
                                if !note.title.is_empty() {
                                    div { class: "chat-paper-title", "{note.title}" }
                                }
                                if !note.body.is_empty() {
                                    p { class: "chat-note-body", "{note.body}" }
                                }
                            }
                        }
                    }
                }
            }
        }
        ResultKind::Annotations => {
            let anns = output
                .as_deref()
                .map(annotations_from_output)
                .unwrap_or_default();
            if anns.is_empty() {
                rsx! { p { class: "chat-tool-confirm", "No annotations." } }
            } else {
                rsx! {
                    div { class: "chat-note-list",
                        for (i, ann) in anns.iter().enumerate() {
                            div { key: "{i}", class: "chat-note-card",
                                div { class: "chat-paper-meta",
                                    span { "Page {ann.page}" }
                                    span { class: "chat-paper-sep", "\u{00b7}" }
                                    span { "{ann.kind}" }
                                }
                                if let Some(content) = ann.content.as_ref() {
                                    p { class: "chat-note-body", "{content}" }
                                }
                            }
                        }
                    }
                }
            }
        }
        ResultKind::Names => {
            let names = output.as_deref().map(names_from_output).unwrap_or_default();
            if names.is_empty() {
                rsx! { p { class: "chat-tool-confirm", "Nothing listed." } }
            } else {
                rsx! {
                    ul { class: "chat-name-list",
                        for (i, name) in names.iter().enumerate() {
                            li { key: "{i}", "{name}" }
                        }
                    }
                }
            }
        }
        ResultKind::Confirmation => {
            let line = confirmation_line(&chip);
            rsx! { p { class: "chat-tool-confirm", "{line}" } }
        }
        ResultKind::Graph => {
            let line = output
                .as_deref()
                .and_then(|s| graph_summary(Some(s)))
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| {
                    if chip.summary.is_empty() {
                        "No connections.".into()
                    } else {
                        chip.summary.clone()
                    }
                });
            rsx! { p { class: "chat-tool-confirm", "{line}" } }
        }
        ResultKind::Relationships => {
            let rels = output
                .as_deref()
                .map(relationships_from_output)
                .unwrap_or_default();
            if rels.is_empty() {
                rsx! { p { class: "chat-tool-confirm", "No related papers." } }
            } else {
                rsx! {
                    ul { class: "chat-name-list",
                        for (i, rel) in rels.iter().enumerate() {
                            li { key: "{i}",
                                span { class: "chat-paper-title", "{rel.title}" }
                                if !rel.label.is_empty() {
                                    span { class: "chat-paper-meta", " · {rel.label}" }
                                }
                            }
                        }
                    }
                }
            }
        }
        ResultKind::PdfUrls => {
            let urls = output.as_deref().map(urls_from_output).unwrap_or_default();
            if urls.is_empty() {
                rsx! { p { class: "chat-tool-confirm", "No PDF found." } }
            } else {
                rsx! {
                    ul { class: "chat-name-list",
                        for (i, url) in urls.iter().enumerate() {
                            li { key: "{i}", class: "chat-resource-uri", "{url}" }
                        }
                    }
                }
            }
        }
        ResultKind::ExtractedText => {
            if let Some(extracted) = output.as_deref().and_then(extracted_text_from_output) {
                let pages = if extracted.total_pages > 0 {
                    format!(
                        "Pages {start}–{end} of {total}",
                        start = extracted.page_start,
                        end = extracted.page_end.max(extracted.page_start),
                        total = extracted.total_pages
                    )
                } else {
                    "Extracted text".into()
                };
                rsx! {
                    div { class: "chat-extract",
                        div { class: "chat-paper-meta", "{pages}" }
                        pre { class: "chat-extract-text", "{extracted.text}" }
                    }
                }
            } else {
                rsx! { p { class: "chat-tool-confirm", "No extracted text." } }
            }
        }
    }
}

fn redact_ids(text: &str) -> String {
    text.split_whitespace()
        .map(|tok| {
            let trimmed = tok.trim_matches(|c: char| !c.is_ascii_hexdigit() && c != '-');
            if looks_like_id(trimmed) { "…" } else { tok }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[component]
fn ChatPaperCard(paper: PaperSnippet) -> Element {
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let title = paper.title.clone();
    let authors = paper.authors.clone();
    let year = paper.year;
    let id = paper.id.clone();
    let in_library = id.as_ref().is_some_and(|id| {
        lib_state
            .read()
            .papers
            .iter()
            .any(|p| p.id.as_deref() == Some(id.as_str()))
    });

    rsx! {
        button {
            class: "chat-paper-card",
            r#type: "button",
            disabled: !in_library,
            onclick: move |_| {
                if let Some(id) = id.clone() {
                    lib_state.with_mut(|s| {
                        if s.papers.iter().any(|p| p.id.as_deref() == Some(id.as_str())) {
                            s.select_one(id);
                        }
                    });
                }
            },
            div { class: "chat-paper-title", "{title}" }
            div { class: "chat-paper-meta",
                span { "{authors}" }
                if let Some(year) = year {
                    span { class: "chat-paper-sep", "\u{00b7}" }
                    span { "{year}" }
                }
            }
        }
    }
}

#[component]
fn ResourceCard(
    title: String,
    uri: String,
    mime_type: Option<String>,
    text: Option<String>,
) -> Element {
    rsx! {
        div { class: "chat-resource-card",
            div { class: "chat-resource-title", "{title}" }
            div { class: "chat-resource-uri", "{uri}" }
            if let Some(mime) = mime_type {
                span { class: "chat-resource-mime", "{mime}" }
            }
            if let Some(text) = text {
                pre { class: "chat-resource-text", "{text}" }
            }
        }
    }
}

#[component]
fn DiffView(path: String, old_text: Option<String>, new_text: String) -> Element {
    let lines = line_diff(old_text.as_deref().unwrap_or(""), &new_text);
    rsx! {
        div { class: "chat-diff",
            div { class: "chat-diff-path", "{path}" }
            div { class: "chat-diff-lines",
                for (i, line) in lines.iter().enumerate() {
                    {
                        let (class, prefix, text) = match line {
                            DiffLine::Equal(t) => ("chat-diff-line chat-diff-line--eq", " ", t.as_str()),
                            DiffLine::Delete(t) => ("chat-diff-line chat-diff-line--del", "-", t.as_str()),
                            DiffLine::Insert(t) => ("chat-diff-line chat-diff-line--add", "+", t.as_str()),
                        };
                        rsx! {
                            div { key: "{i}", class: "{class}",
                                span { class: "chat-diff-prefix", "{prefix}" }
                                span { "{text}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn JsonBlock(value: serde_json::Value, copied: bool, on_copy: EventHandler<()>) -> Element {
    let pretty = serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string());
    let tokens = tokenize_json(&pretty);
    rsx! {
        div { class: "chat-json-box",
            button {
                class: "chat-json-copy",
                r#type: "button",
                onclick: move |_| on_copy.call(()),
                if copied { "Copied" } else { "Copy" }
            }
            pre { class: "chat-json",
                for (i, (class, text)) in tokens.iter().enumerate() {
                    span { key: "{i}", class: "{class}", "{text}" }
                }
            }
        }
    }
}

fn kind_icon(kind: ToolKind) -> &'static str {
    match kind {
        ToolKind::Read => "bi bi-file-text",
        ToolKind::Edit => "bi bi-pencil",
        ToolKind::Delete => "bi bi-trash",
        ToolKind::Move => "bi bi-arrow-left-right",
        ToolKind::Search => "bi bi-search",
        ToolKind::Execute => "bi bi-terminal",
        ToolKind::Think => "bi bi-lightbulb",
        ToolKind::Fetch => "bi bi-cloud-arrow-down",
        ToolKind::SwitchMode => "bi bi-toggles",
        ToolKind::Other => "bi bi-wrench",
    }
}

fn kind_label(kind: ToolKind) -> &'static str {
    match kind {
        ToolKind::Read => "Read",
        ToolKind::Edit => "Edit",
        ToolKind::Delete => "Delete",
        ToolKind::Move => "Move",
        ToolKind::Search => "Search",
        ToolKind::Execute => "Run",
        ToolKind::Think => "Think",
        ToolKind::Fetch => "Fetch",
        ToolKind::SwitchMode => "Mode",
        ToolKind::Other => "Tool",
    }
}

pub(crate) fn input_summary(title: &str, raw_input: &Option<serde_json::Value>) -> String {
    let arg = raw_input.as_ref().and_then(first_summary_arg);
    match arg {
        Some(s) => {
            let clipped = truncate(&s, 40);
            if title
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
            {
                format!("{title}(\"{clipped}\")")
            } else {
                format!("{title}  \"{clipped}\"")
            }
        }
        None => title.to_string(),
    }
}

fn first_summary_arg(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(s) if !s.is_empty() => Some(s.clone()),
        serde_json::Value::Object(map) => {
            const KEYS: &[&str] = &[
                "query", "q", "path", "uri", "url", "id", "paper_id", "command", "name", "title",
                "text", "prompt",
            ];
            for key in KEYS {
                if let Some(s) = map
                    .get(*key)
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                {
                    return Some(s.to_string());
                }
            }
            map.values()
                .find_map(|v| v.as_str().filter(|s| !s.is_empty()).map(str::to_string))
        }
        _ => None,
    }
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

fn tokenize_json(s: &str) -> Vec<(&'static str, String)> {
    let chars: Vec<char> = s.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            let start = i;
            while i < chars.len() && chars[i].is_whitespace() {
                i += 1;
            }
            out.push(("json-space", chars[start..i].iter().collect()));
        } else if c == '"' {
            let start = i;
            i += 1;
            while i < chars.len() {
                if chars[i] == '\\' {
                    i = (i + 2).min(chars.len());
                    continue;
                }
                if chars[i] == '"' {
                    i += 1;
                    break;
                }
                i += 1;
            }
            let string: String = chars[start..i].iter().collect();
            let mut j = i;
            while j < chars.len() && chars[j].is_whitespace() {
                j += 1;
            }
            let class = if j < chars.len() && chars[j] == ':' {
                "json-key"
            } else {
                "json-string"
            };
            out.push((class, string));
        } else if c.is_ascii_digit()
            || (c == '-' && chars.get(i + 1).is_some_and(|n| n.is_ascii_digit()))
        {
            let start = i;
            i += 1;
            while i < chars.len()
                && (chars[i].is_ascii_digit() || matches!(chars[i], '.' | 'e' | 'E' | '+' | '-'))
            {
                i += 1;
            }
            out.push(("json-number", chars[start..i].iter().collect()));
        } else if starts_with(&chars, i, "true") {
            out.push(("json-literal", "true".into()));
            i += 4;
        } else if starts_with(&chars, i, "false") {
            out.push(("json-literal", "false".into()));
            i += 5;
        } else if starts_with(&chars, i, "null") {
            out.push(("json-literal", "null".into()));
            i += 4;
        } else {
            out.push(("json-punct", c.to_string()));
            i += 1;
        }
    }
    out
}

fn starts_with(chars: &[char], at: usize, word: &str) -> bool {
    let end = at + word.len();
    if end > chars.len() {
        return false;
    }
    chars[at..end].iter().copied().eq(word.chars())
        && chars.get(end).is_none_or(|c| !c.is_ascii_alphanumeric())
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum DiffLine {
    Equal(String),
    Delete(String),
    Insert(String),
}

fn line_diff(old: &str, new: &str) -> Vec<DiffLine> {
    let a: Vec<&str> = if old.is_empty() {
        Vec::new()
    } else {
        old.lines().collect()
    };
    let b: Vec<&str> = if new.is_empty() {
        Vec::new()
    } else {
        new.lines().collect()
    };
    if a.is_empty() {
        return b
            .into_iter()
            .map(|l| DiffLine::Insert(l.to_string()))
            .collect();
    }
    if b.is_empty() {
        return a
            .into_iter()
            .map(|l| DiffLine::Delete(l.to_string()))
            .collect();
    }
    if a.len().saturating_mul(b.len()) > 160_000 {
        let mut out: Vec<DiffLine> = a
            .iter()
            .map(|l| DiffLine::Delete((*l).to_string()))
            .collect();
        out.extend(b.iter().map(|l| DiffLine::Insert((*l).to_string())));
        return out;
    }
    let n = a.len();
    let m = b.len();
    let mut dp = vec![vec![0u32; m + 1]; n + 1];
    for i in 0..n {
        for j in 0..m {
            dp[i + 1][j + 1] = if a[i] == b[j] {
                dp[i][j] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }
    let mut rev = Vec::new();
    let mut i = n;
    let mut j = m;
    while i > 0 && j > 0 {
        if a[i - 1] == b[j - 1] {
            rev.push(DiffLine::Equal(a[i - 1].to_string()));
            i -= 1;
            j -= 1;
        } else if dp[i][j - 1] >= dp[i - 1][j] {
            rev.push(DiffLine::Insert(b[j - 1].to_string()));
            j -= 1;
        } else {
            rev.push(DiffLine::Delete(a[i - 1].to_string()));
            i -= 1;
        }
    }
    while i > 0 {
        rev.push(DiffLine::Delete(a[i - 1].to_string()));
        i -= 1;
    }
    while j > 0 {
        rev.push(DiffLine::Insert(b[j - 1].to_string()));
        j -= 1;
    }
    rev.reverse();
    rev
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_summary_formats_search_query() {
        assert_eq!(
            input_summary(
                "search_papers",
                &Some(serde_json::json!({"query": "attention"}))
            ),
            "search_papers(\"attention\")"
        );
    }

    #[test]
    fn input_summary_falls_back_to_title() {
        assert_eq!(input_summary("search_papers", &None), "search_papers");
    }

    #[test]
    fn line_diff_marks_inserts_and_deletes() {
        let lines = line_diff("a\nb\nc", "a\nx\nc");
        assert_eq!(
            lines,
            vec![
                DiffLine::Equal("a".into()),
                DiffLine::Delete("b".into()),
                DiffLine::Insert("x".into()),
                DiffLine::Equal("c".into()),
            ]
        );
    }

    #[test]
    fn tokenize_json_marks_keys_and_strings() {
        let tokens = tokenize_json("{\"a\": \"b\"}");
        let classes: Vec<_> = tokens.iter().map(|(c, _)| *c).collect();
        assert!(classes.contains(&"json-key"));
        assert!(classes.contains(&"json-string"));
    }
}
