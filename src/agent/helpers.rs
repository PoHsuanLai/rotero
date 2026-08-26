use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;

use super::LoopResult;
use super::install::{find_mcp_binary, find_pdfium_path};
use super::types::{AgentAuthMethod, AgentModel, ChatEvent, ChatRequest, SlashCommand, ToolStatus};

/// Builds a `Command` for an executable that may be a Windows batch file.
///
/// `npm` ships as `npm.cmd` on Windows, and `CreateProcess` cannot run a batch
/// file directly — it has to go through `cmd.exe /C`. Everywhere else, and for
/// real executables on Windows, the program is invoked directly.
pub(crate) fn command_for_program(program: &Path) -> Command {
    std::cfg_select! {
        windows => {
            if is_batch_file(program) {
                let mut cmd = Command::new("cmd");
                cmd.arg("/C").arg(program);
                cmd
            } else {
                Command::new(program)
            }
        }
        _ => Command::new(program),
    }
}

/// Whether `program` is a Windows batch file, which `CreateProcess` refuses to
/// execute directly. Defined on all platforms so it stays unit-testable; only
/// the Windows arm of `command_for_program` calls it.
#[cfg_attr(not(windows), allow(dead_code))]
fn is_batch_file(program: &Path) -> bool {
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

pub(crate) fn build_mcp_servers_json() -> serde_json::Value {
    #[cfg(feature = "desktop")]
    if let Some(&port) = crate::MCP_HTTP_PORT.get() {
        let url = format!("http://127.0.0.1:{port}/mcp");
        tracing::info!("MCP: using embedded HTTP server at {url}");
        return serde_json::json!([{
            "type": "http",
            "name": "rotero",
            "url": url,
            "headers": [],
        }]);
    }

    let mcp_binary = find_mcp_binary();
    let pdfium_path = find_pdfium_path();

    if let Some(mcp_bin) = &mcp_binary {
        tracing::info!("MCP: using stdio binary at {}", mcp_bin.display());
        serde_json::json!([{
            "type": "stdio",
            "name": "rotero",
            "command": mcp_bin.to_string_lossy(),
            "args": [],
            "env": [{
                "name": "PDFIUM_DYNAMIC_LIB_PATH",
                "value": pdfium_path.as_ref().map(|p| p.to_string_lossy().to_string()).unwrap_or_default()
            }]
        }])
    } else {
        tracing::warn!("MCP: no server available — agent won't have library tools");
        serde_json::json!([])
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

pub(crate) fn handle_notification(
    evt_tx: &tokio::sync::mpsc::UnboundedSender<ChatEvent>,
    v: &serde_json::Value,
) {
    let method = v.get("method").and_then(|m| m.as_str()).unwrap_or("");

    match method {
        "sessionUpdate" | "session/update" => {
            let params = match v.get("params") {
                Some(p) => p,
                None => return,
            };
            let update = match params.get("update") {
                Some(u) => u,
                None => return,
            };
            let update_type = update
                .get("sessionUpdate")
                .and_then(|u| u.as_str())
                .unwrap_or("");

            match update_type {
                "user_message_chunk" => {
                    if let Some(text) = update
                        .get("content")
                        .and_then(|c| c.get("text"))
                        .and_then(|t| t.as_str())
                    {
                        // A user chunk is a whole message, so the padding
                        // left behind by tag removal can go.
                        let cleaned = strip_protocol_tags(text).trim().to_string();
                        if !cleaned.is_empty() {
                            let _ = evt_tx.send(ChatEvent::UserMessage(cleaned));
                        }
                    }
                }
                "agent_message_chunk" => {
                    if let Some(text) = update
                        .get("content")
                        .and_then(|c| c.get("text"))
                        .and_then(|t| t.as_str())
                    {
                        // Chunks are concatenated downstream, so whitespace
                        // at the boundary is load-bearing markdown structure.
                        let cleaned = strip_protocol_tags(text);
                        if !cleaned.is_empty() {
                            let _ = evt_tx.send(ChatEvent::TextDelta(cleaned));
                        }
                    }
                }
                "tool_call" => {
                    let id = update
                        .get("toolCallId")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let title = update
                        .get("title")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let _ = evt_tx.send(ChatEvent::ToolCallStarted { id, title });
                }
                "tool_call_update" => {
                    let id = update
                        .get("toolCallId")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let status = match update.get("status").and_then(|s| s.as_str()) {
                        Some("pending") => ToolStatus::Pending,
                        Some("in_progress") => ToolStatus::InProgress,
                        Some("completed") => ToolStatus::Completed,
                        Some("failed") => ToolStatus::Failed,
                        _ => return,
                    };
                    let output = update
                        .get("content")
                        .and_then(|c| c.as_array())
                        .and_then(|arr| {
                            let texts: Vec<String> = arr
                                .iter()
                                .filter_map(|item| {
                                    item.get("text")
                                        .and_then(|t| t.as_str())
                                        .map(String::from)
                                        .or_else(|| {
                                            item.get("content")
                                                .and_then(|c| c.get("text"))
                                                .and_then(|t| t.as_str())
                                                .map(String::from)
                                        })
                                })
                                .collect();
                            if texts.is_empty() {
                                None
                            } else {
                                Some(texts.join("\n"))
                            }
                        });
                    let _ = evt_tx.send(ChatEvent::ToolCallUpdated { id, status, output });
                }
                "available_commands_update" => {
                    let commands = update
                        .get("availableCommands")
                        .and_then(|c| c.as_array())
                        .map(|arr| {
                            arr.iter()
                                .map(|c| SlashCommand {
                                    name: c
                                        .get("name")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("")
                                        .to_string(),
                                    description: c
                                        .get("description")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("")
                                        .to_string(),
                                    hint: c
                                        .get("input")
                                        .and_then(|i| i.get("hint"))
                                        .and_then(|h| h.as_str())
                                        .map(String::from),
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    let _ = evt_tx.send(ChatEvent::CommandsAvailable(commands));
                }
                _ => {}
            }
        }
        "session/requestPermission" => {
            if let Some(id) = v.get("id") {
                tracing::debug!("ACP: auto-allowing permission request {id}");
            }
        }
        _ => {}
    }
}

pub(crate) fn api_key_env_for_method(method_id: &str) -> Option<String> {
    match method_id {
        "gemini-api-key" => Some("GEMINI_API_KEY".into()),
        "codex-api-key" | "openai-api-key" => Some("OPENAI_API_KEY".into()),
        "codex_api_key" => Some("CODEX_API_KEY".into()),
        id if id.contains("api-key") || id.contains("api_key") => {
            Some(id.to_uppercase().replace('-', "_"))
        }
        _ => None,
    }
}

pub(crate) fn extract_permission_options(v: &serde_json::Value) -> Vec<(String, String)> {
    v.get("params")
        .and_then(|p| p.get("options"))
        .and_then(|o| o.as_array())
        .map(|arr| {
            arr.iter()
                .map(|opt| {
                    let id = opt
                        .get("optionId")
                        .and_then(|v| v.as_str())
                        .unwrap_or("default")
                        .to_string();
                    let label = opt
                        .get("label")
                        .and_then(|v| v.as_str())
                        .or_else(|| opt.get("name").and_then(|v| v.as_str()))
                        .unwrap_or(&id)
                        .to_string();
                    (id, label)
                })
                .collect()
        })
        .unwrap_or_else(|| vec![("default".into(), "Allow".into())])
}

pub(crate) fn first_allow_option_id(v: &serde_json::Value) -> String {
    v.get("params")
        .and_then(|p| p.get("options"))
        .and_then(|o| o.as_array())
        .and_then(|arr| {
            arr.iter()
                .find(|opt| {
                    let kind = opt.get("kind").and_then(|k| k.as_str()).unwrap_or("");
                    kind.contains("allow") || kind == "default"
                })
                .or_else(|| arr.first())
        })
        .and_then(|opt| opt.get("optionId").and_then(|id| id.as_str()))
        .unwrap_or("default")
        .to_string()
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

pub(crate) fn extract_models_event(models: &serde_json::Value) -> ChatEvent {
    let available = models
        .get("availableModels")
        .and_then(|m| m.as_array())
        .map(|arr| {
            arr.iter()
                .map(|m| AgentModel {
                    id: m
                        .get("modelId")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    name: m
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    description: m
                        .get("description")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                })
                .collect()
        })
        .unwrap_or_default();

    let current = models
        .get("currentModelId")
        .and_then(|v| v.as_str())
        .unwrap_or("default")
        .to_string();

    ChatEvent::ModelsAvailable {
        models: available,
        current,
    }
}

pub(crate) fn extract_auth_methods(init_result: &serde_json::Value) -> Vec<AgentAuthMethod> {
    init_result
        .get("authMethods")
        .and_then(|m| m.as_array())
        .map(|arr| {
            arr.iter()
                .map(|m| {
                    let (terminal_command, terminal_args) = m
                        .get("_meta")
                        .and_then(|meta| meta.get("terminal-auth"))
                        .map(|ta| {
                            let cmd = ta
                                .get("command")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string();
                            let args: Vec<String> = ta
                                .get("args")
                                .and_then(|v| v.as_array())
                                .map(|arr| {
                                    arr.iter()
                                        .filter_map(|v| v.as_str().map(String::from))
                                        .collect()
                                })
                                .unwrap_or_default();
                            (Some(cmd), args)
                        })
                        .unwrap_or((None, vec![]));

                    AgentAuthMethod {
                        id: m
                            .get("id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        name: m
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        description: m
                            .get("description")
                            .and_then(|v| v.as_str())
                            .map(String::from),
                        terminal_command,
                        terminal_args,
                        is_api_key: m
                            .get("_meta")
                            .and_then(|meta| meta.get("api-key"))
                            .is_some(),
                        api_key_env_var: api_key_env_for_method(
                            m.get("id").and_then(|v| v.as_str()).unwrap_or(""),
                        ),
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

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
