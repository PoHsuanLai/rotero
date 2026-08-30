use std::path::{Path, PathBuf};
use std::sync::mpsc;

use agent_client_protocol::schema::v1::{
    AuthMethod, ContentBlock, McpServer, McpServerHttp, McpServerStdio, SessionConfigKind,
    SessionConfigOption, SessionConfigOptionCategory, SessionConfigSelectOptions, SessionUpdate,
    ToolCall, ToolCallContent, ToolCallStatus, ToolCallUpdate,
};

use super::LoopResult;
use super::install::{find_mcp_binary, find_pdfium_path};
use super::types::{AgentAuthMethod, AgentModel, ChatEvent, ChatRequest, SlashCommand, ToolStatus};

/// Whether `program` is a Windows batch file, which `CreateProcess` refuses to
/// execute directly. Defined on all platforms so it stays unit-testable;
/// `launch::npx_launch` wraps matching paths in `cmd /C`.
pub(crate) fn is_batch_file(program: &Path) -> bool {
    program
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("cmd") || e.eq_ignore_ascii_case("bat"))
}

pub(crate) fn agent_working_dir() -> PathBuf {
    #[cfg(feature = "desktop")]
    {
        directories::BaseDirs::new()
            .map(|d| d.data_dir().join("com.rotero.Rotero"))
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
    }
    #[cfg(not(feature = "desktop"))]
    {
        std::env::current_dir().unwrap_or_default()
    }
}

pub(crate) fn build_mcp_servers() -> Vec<McpServer> {
    #[cfg(feature = "desktop")]
    if let Some(&port) = crate::MCP_HTTP_PORT.get() {
        let url = format!("http://127.0.0.1:{port}/mcp");
        tracing::info!("MCP: using embedded HTTP server at {url}");
        return vec![McpServer::Http(McpServerHttp::new("rotero", url))];
    }

    let mcp_binary = find_mcp_binary();
    let pdfium_path = find_pdfium_path();

    if let Some(mcp_bin) = mcp_binary {
        tracing::info!("MCP: using stdio binary at {}", mcp_bin.display());
        let env = match pdfium_path {
            Some(p) => vec![agent_client_protocol::schema::v1::EnvVariable::new(
                "PDFIUM_DYNAMIC_LIB_PATH",
                p.to_string_lossy().into_owned(),
            )],
            None => vec![],
        };
        vec![McpServer::Stdio(
            McpServerStdio::new("rotero", mcp_bin).env(env),
        )]
    } else {
        tracing::warn!("MCP: no server available — agent won't have library tools");
        vec![]
    }
}

pub(crate) fn is_auth_error(err: &str) -> bool {
    let lower = err.to_lowercase();
    lower.contains("authentication required")
        || lower.contains("auth_required")
        || lower.contains("api key")
        || lower.contains("not configured")
        || lower.contains("not authenticated")
        || lower.contains("login required")
        || lower.contains("unauthorized")
        || lower.contains("credentials")
}

pub(crate) fn wait_for_switch_or_shutdown(req_rx: &mpsc::Receiver<ChatRequest>) -> LoopResult {
    loop {
        match req_rx.recv() {
            Ok(ChatRequest::SwitchAgent { provider_id }) => {
                return LoopResult::SwitchAgent(provider_id);
            }
            Ok(ChatRequest::Shutdown) => return LoopResult::Shutdown,
            Err(_) => return LoopResult::Shutdown,
            _ => {}
        }
    }
}

pub(crate) fn api_key_env_for_method(method_id: &str) -> Option<String> {
    match method_id {
        "xai.api_key" | "xai-api-key" => Some("XAI_API_KEY".into()),
        "codex-api-key" | "openai-api-key" => Some("OPENAI_API_KEY".into()),
        "codex_api_key" => Some("CODEX_API_KEY".into()),
        "anthropic-api-key" | "claude-api-key" => Some("ANTHROPIC_API_KEY".into()),
        id if id.contains("api-key") || id.contains("api_key") => {
            Some(id.to_uppercase().replace(['-', '.'], "_"))
        }
        _ => None,
    }
}

pub(crate) fn auth_methods_from_acp(methods: &[AuthMethod]) -> Vec<AgentAuthMethod> {
    methods
        .iter()
        .map(|m| {
            let id = m.id().0.to_string();
            let api_key_env_var = api_key_env_for_method(&id);
            AgentAuthMethod {
                id: id.clone(),
                name: m.name().to_string(),
                description: m.description().map(str::to_string),
                is_api_key: api_key_env_var.is_some(),
                api_key_env_var,
            }
        })
        .collect()
}

pub(crate) fn models_from_config_options(
    options: &[SessionConfigOption],
) -> Option<(Vec<AgentModel>, String, String)> {
    let option = options.iter().find(|o| {
        matches!(o.category, Some(SessionConfigOptionCategory::Model))
            || o.id.0.as_ref() == "model"
    })?;
    let SessionConfigKind::Select(select) = &option.kind else {
        return None;
    };
    let models = match &select.options {
        SessionConfigSelectOptions::Ungrouped(opts) => opts
            .iter()
            .map(|o| AgentModel {
                id: o.value.0.to_string(),
                name: o.name.clone(),
                description: o.description.clone().unwrap_or_default(),
            })
            .collect(),
        SessionConfigSelectOptions::Grouped(groups) => groups
            .iter()
            .flat_map(|g| g.options.iter())
            .map(|o| AgentModel {
                id: o.value.0.to_string(),
                name: o.name.clone(),
                description: o.description.clone().unwrap_or_default(),
            })
            .collect(),
        _ => Vec::new(),
    };
    Some((
        models,
        select.current_value.0.to_string(),
        option.id.0.to_string(),
    ))
}

fn content_block_text(block: &ContentBlock) -> Option<&str> {
    match block {
        ContentBlock::Text(t) => Some(t.text.as_str()),
        _ => None,
    }
}

fn tool_status(status: ToolCallStatus) -> ToolStatus {
    match status {
        ToolCallStatus::Pending => ToolStatus::Pending,
        ToolCallStatus::InProgress => ToolStatus::InProgress,
        ToolCallStatus::Completed => ToolStatus::Completed,
        ToolCallStatus::Failed => ToolStatus::Failed,
        _ => ToolStatus::InProgress,
    }
}

fn tool_output_text(content: &[ToolCallContent]) -> Option<String> {
    let texts: Vec<String> = content
        .iter()
        .filter_map(|item| match item {
            ToolCallContent::Content(c) => content_block_text(&c.content).map(str::to_string),
            _ => None,
        })
        .collect();
    if texts.is_empty() {
        None
    } else {
        Some(texts.join("\n"))
    }
}

pub(crate) fn session_update_to_events(update: &SessionUpdate) -> Vec<ChatEvent> {
    match update {
        SessionUpdate::UserMessageChunk(chunk) => {
            let Some(text) = content_block_text(&chunk.content) else {
                return vec![];
            };
            let context_paper_ids = paper_ids_from_context_block(text);
            let cleaned = strip_protocol_tags(text).trim().to_string();
            if cleaned.is_empty() && context_paper_ids.is_empty() {
                vec![]
            } else {
                vec![ChatEvent::UserMessage {
                    text: cleaned,
                    context_paper_ids,
                }]
            }
        }
        SessionUpdate::AgentMessageChunk(chunk) => {
            let Some(text) = content_block_text(&chunk.content) else {
                return vec![];
            };
            let cleaned = strip_protocol_tags(text);
            if cleaned.is_empty() {
                vec![]
            } else {
                vec![ChatEvent::TextDelta(cleaned)]
            }
        }
        SessionUpdate::ToolCall(ToolCall {
            tool_call_id,
            title,
            ..
        }) => vec![ChatEvent::ToolCallStarted {
            id: tool_call_id.0.to_string(),
            title: title.clone(),
        }],
        SessionUpdate::ToolCallUpdate(ToolCallUpdate {
            tool_call_id,
            fields,
            ..
        }) => {
            let Some(status) = fields.status else {
                return vec![];
            };
            vec![ChatEvent::ToolCallUpdated {
                id: tool_call_id.0.to_string(),
                status: tool_status(status),
                output: fields.content.as_deref().and_then(tool_output_text),
            }]
        }
        SessionUpdate::AvailableCommandsUpdate(update) => {
            let commands = update
                .available_commands
                .iter()
                .map(|c| SlashCommand {
                    name: c.name.clone(),
                    description: c.description.clone(),
                    hint: match &c.input {
                        Some(
                            agent_client_protocol::schema::v1::AvailableCommandInput::Unstructured(
                                input,
                            ),
                        ) => Some(input.hint.clone()),
                        _ => None,
                    },
                })
                .collect();
            vec![ChatEvent::CommandsAvailable(commands)]
        }
        SessionUpdate::ConfigOptionUpdate(update) => {
            models_from_config_options(&update.config_options)
                .map(|(models, current, config_id)| ChatEvent::ModelsAvailable {
                    models,
                    current,
                    config_id: Some(config_id),
                })
                .into_iter()
                .collect()
        }
        _ => vec![],
    }
}

/// Remove protocol tags and their contents, leaving all other whitespace
/// untouched.
///
/// Whitespace-neutral by contract: agent replies arrive as a stream of chunks
/// that are concatenated before rendering, and markdown is line-oriented, so
/// trimming here would eat the newlines separating blocks at a chunk boundary
/// — silently demoting headings and dissolving tables. Callers holding a whole
/// message may trim the result themselves.
pub(crate) fn strip_protocol_tags(text: &str) -> String {
    let tag_patterns = [
        "command-name",
        "command-message",
        "command-args",
        "local-command-stdout",
        "local-command-stderr",
        "local-command-caveat",
        "system-reminder",
        "task-notification",
        "task-id",
        "tool-use-id",
        "output-file",
        "status",
        "summary",
        "rotero-context",
    ];

    let mut result = text.to_string();
    for tag in &tag_patterns {
        let open = format!("<{tag}");
        let close = format!("</{tag}>");
        while let Some(start) = result.find(&open) {
            if let Some(end) = result[start..].find(&close) {
                result = format!(
                    "{}{}",
                    &result[..start],
                    &result[start + end + close.len()..]
                );
            } else if let Some(end) = result[start..].find('>') {
                result = format!("{}{}", &result[..start], &result[start + end + 1..]);
            } else {
                break;
            }
        }
    }

    result
}

/// Paper ids named in a message's `<rotero-context>` blocks, in order.
///
/// Runs here rather than in the app layer because [`strip_protocol_tags`]
/// removes the block before a message is ever emitted: the agent's stored
/// transcript is the only record of which paper a past conversation was about,
/// and it is readable exactly once, as the transcript replays on `session/load`.
pub(crate) fn paper_ids_from_context_block(raw: &str) -> Vec<String> {
    const OPEN: &str = "<rotero-context";
    const CLOSE: &str = "</rotero-context>";
    const KEY: &str = "Paper ID:";

    let mut ids = Vec::new();
    let mut rest = raw;
    while let Some(start) = rest.find(OPEN) {
        let after_open = &rest[start..];
        // An unterminated block still carries a usable id, so scan to the end.
        let (block, consumed) = match after_open.find(CLOSE) {
            Some(end) => (&after_open[..end], start + end + CLOSE.len()),
            None => (after_open, rest.len()),
        };
        for line in block.lines() {
            if let Some(value) = line.trim().strip_prefix(KEY) {
                let id = value.trim();
                if !id.is_empty() && !ids.iter().any(|seen| seen == id) {
                    ids.push(id.to_string());
                }
            }
        }
        rest = &rest[consumed..];
    }
    ids
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The exact shape `build_paper_context` produces.
    #[test]
    fn reads_the_paper_id_the_app_sends() {
        let raw = "<rotero-context>\nWhen finding papers, prefer the rotero MCP tools.\n\
                   I'm currently looking at this paper in my library:\n\
                   Title: Attention Is All You Need\nAuthors: Vaswani\nYear: 2017\n\
                   DOI: 10.5555/1\nPaper ID: abc-123\n\
                   You can use the rotero MCP tools.\n</rotero-context>\n\nWhat is this about?";
        assert_eq!(paper_ids_from_context_block(raw), vec!["abc-123"]);
    }

    /// The variant the paper detail panel sends when asking for an OA PDF.
    #[test]
    fn reads_the_shorter_block_from_the_detail_panel() {
        let raw = "<rotero-context>\nPaper ID: xyz-789\nTitle: A\n</rotero-context>";
        assert_eq!(paper_ids_from_context_block(raw), vec!["xyz-789"]);
    }

    #[test]
    fn a_message_without_a_block_names_no_papers() {
        assert!(paper_ids_from_context_block("Just a question.").is_empty());
        assert!(paper_ids_from_context_block("").is_empty());
    }

    #[test]
    fn reads_every_block_and_records_each_paper_once() {
        let raw = "<rotero-context>\nPaper ID: a\n</rotero-context>\
                   <rotero-context>\nPaper ID: b\n</rotero-context>\
                   <rotero-context>\nPaper ID: a\n</rotero-context>";
        assert_eq!(paper_ids_from_context_block(raw), vec!["a", "b"]);
    }

    /// A block cut off mid-stream still names the paper it was about.
    #[test]
    fn an_unterminated_block_still_yields_its_id() {
        assert_eq!(
            paper_ids_from_context_block("<rotero-context>\nPaper ID: a\n"),
            vec!["a"]
        );
    }

    #[test]
    fn recognises_windows_batch_extensions() {
        assert!(is_batch_file(Path::new("C:/node/npm.cmd")));
        assert!(is_batch_file(Path::new("npm.bat")));
        // Windows extensions are case-insensitive.
        assert!(is_batch_file(Path::new("NPM.CMD")));
    }

    #[test]
    fn real_executables_are_not_batch_files() {
        assert!(!is_batch_file(Path::new("C:/node/node.exe")));
        assert!(!is_batch_file(Path::new("/usr/local/bin/node")));
        assert!(!is_batch_file(Path::new("npm")));
        // A name merely containing "cmd" is not a batch file.
        assert!(!is_batch_file(Path::new("/usr/bin/cmdline")));
    }

    #[test]
    fn maps_xai_api_key_env() {
        assert_eq!(
            api_key_env_for_method("xai.api_key").as_deref(),
            Some("XAI_API_KEY")
        );
        assert_eq!(api_key_env_for_method("gemini-api-key").as_deref(), Some("GEMINI_API_KEY"));
    }
}

/// Streaming-fidelity tests over the markdown shapes agent replies use
/// (`tests/fixtures/chat_markdown/`).
///
/// The chat panel renders assistant text by concatenating `TextDelta` chunks
/// and handing the result to `md_to_html`. Markdown is line-oriented, so the
/// concatenated text must reproduce the agent's original bytes exactly:
/// a lost newline silently demotes a heading or dissolves a table.
#[cfg(test)]
mod streaming_markdown {
    use super::*;

    fn fixtures() -> Vec<(String, String)> {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/chat_markdown");
        let mut out: Vec<(String, String)> = std::fs::read_dir(dir)
            .expect("fixture directory")
            .filter_map(|e| {
                let path = e.ok()?.path();
                if path.extension()? != "md" {
                    return None;
                }
                let name = path.file_name()?.to_string_lossy().into_owned();
                Some((name, std::fs::read_to_string(&path).ok()?))
            })
            .collect();
        out.sort();
        assert!(!out.is_empty(), "no fixtures found");
        out
    }

    /// Reassemble a reply the way the panel does: every chunk passes through
    /// `strip_protocol_tags`, then the results are concatenated.
    fn stream(text: &str, chunk: usize) -> String {
        let chars: Vec<char> = text.chars().collect();
        let mut out = String::new();
        for piece in chars.chunks(chunk) {
            out.push_str(&strip_protocol_tags(&piece.iter().collect::<String>()));
        }
        out
    }

    /// Reassemble a reply split at exactly one boundary. The ACP stream can
    /// break anywhere, so every position is a case worth covering.
    fn stream_split_at(text: &str, at: usize) -> String {
        let chars: Vec<char> = text.chars().collect();
        let (head, tail) = chars.split_at(at);
        let mut out = strip_protocol_tags(&head.iter().collect::<String>());
        out.push_str(&strip_protocol_tags(&tail.iter().collect::<String>()));
        out
    }

    /// Chunk sizes spanning single characters up to the whole reply.
    fn chunk_sizes(text: &str) -> Vec<usize> {
        let len = text.chars().count().max(1);
        [1, 2, 7, 40, 200, 1024]
            .into_iter()
            .filter(|&n| n < len)
            .chain(std::iter::once(len))
            .collect()
    }

    /// Chunk boundaries are a transport artifact: where the ACP stream happens
    /// to split a reply must not change the text the renderer receives.
    #[test]
    fn chunk_boundaries_preserve_reply_text() {
        for (name, text) in fixtures() {
            for chunk in chunk_sizes(&text) {
                assert_eq!(
                    stream(&text, chunk),
                    text,
                    "{name}: reassembly at chunk size {chunk} does not match the original reply"
                );
            }
            // Every possible single split point, not just the sampled sizes.
            for at in 0..=text.chars().count() {
                assert_eq!(
                    stream_split_at(&text, at),
                    text,
                    "{name}: reassembly with a chunk boundary at {at} does not match the original reply"
                );
            }
        }
    }

    /// A heading only parses when its `#` starts a line. Losing the newline
    /// before it turns the heading into body text.
    #[test]
    fn streamed_headings_still_render_as_headings() {
        let mut covered = 0;
        for (name, text) in fixtures() {
            let expected = crate::ui::markdown::md_to_html(&text);
            if !expected.contains("<h1>")
                && !expected.contains("<h2>")
                && !expected.contains("<h3>")
            {
                continue;
            }
            covered += 1;
            for chunk in chunk_sizes(&text) {
                let got = crate::ui::markdown::md_to_html(&stream(&text, chunk));
                assert_eq!(
                    got, expected,
                    "{name}: headings lost when streamed in {chunk}-char chunks"
                );
            }
        }
        assert!(covered > 0, "no fixture exercises headings");
    }

    /// Tables need every row on its own line, so they are the most fragile
    /// construct in the stream.
    #[test]
    fn streamed_tables_still_render_as_tables() {
        let mut covered = 0;
        for (name, text) in fixtures() {
            let expected = crate::ui::markdown::md_to_html(&text);
            if !expected.contains("<table>") {
                continue;
            }
            covered += 1;
            for chunk in chunk_sizes(&text) {
                let got = crate::ui::markdown::md_to_html(&stream(&text, chunk));
                assert!(
                    got.contains("<table>"),
                    "{name}: table lost when streamed in {chunk}-char chunks"
                );
                assert_eq!(
                    got, expected,
                    "{name}: table content changed when streamed in {chunk}-char chunks"
                );
            }
        }
        assert!(covered > 0, "no fixture exercises tables");
    }

    /// Tag stripping is the only transformation the text is meant to undergo;
    /// it must not also rewrite the surrounding markdown.
    #[test]
    fn stripping_tags_leaves_surrounding_markdown_intact() {
        let text = "## Heading\n\n<system-reminder>ignore me</system-reminder>\n\n| a | b |\n|---|---|\n| 1 | 2 |\n";
        let stripped = strip_protocol_tags(text);
        assert!(stripped.contains("## Heading"));
        assert!(!stripped.contains("ignore me"));
        let html = crate::ui::markdown::md_to_html(&stripped);
        assert!(html.contains("<h2>"), "heading survives tag stripping");
        assert!(html.contains("<table>"), "table survives tag stripping");
    }

    /// Interior whitespace carries meaning in markdown; only the reply's outer
    /// padding is safe to drop.
    #[test]
    fn stripping_tags_preserves_interior_newlines() {
        assert_eq!(strip_protocol_tags("a\n\nb"), "a\n\nb");
        assert_eq!(
            strip_protocol_tags("| a |\n"),
            "| a |\n",
            "a chunk's trailing newline starts the next table row"
        );
        assert_eq!(
            strip_protocol_tags("\n\n## H"),
            "\n\n## H",
            "a chunk's leading newline ends the previous block"
        );
        // Only a caller holding a whole message may trim.
        assert_eq!(strip_protocol_tags("\n\n## H\n").trim(), "## H");
    }
}
