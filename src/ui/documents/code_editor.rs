//! CodeMirror-backed source editor for the Documents panel.
//!
//! Wraps the vendored CodeMirror 6 bundle (`assets/editor.js`, exposed as
//! `window.__roteroEditor`) behind a Dioxus component. It mounts an editor into
//! a mount div, streams edits back through the same long-lived-eval + event-queue
//! bridge the graph view uses, and reconfigures language/content reactively.

use dioxus::prelude::*;

/// Language mode for the editor's syntax highlighting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorLanguage {
    /// Typst source (the paper-authoring surface).
    Typst,
    /// Markdown (the quick-summary / agent surface).
    Markdown,
}

impl EditorLanguage {
    /// The identifier understood by `window.__roteroEditor`.
    fn js_id(self) -> &'static str {
        match self {
            EditorLanguage::Typst => "typst",
            EditorLanguage::Markdown => "markdown",
        }
    }
}

/// Emit a JSON string literal (quotes included) for safe injection into an eval
/// snippet — covers quotes, backslashes, newlines, and control characters.
fn js_string(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".to_string())
}

/// A syntax-highlighted source editor.
///
/// - `value` seeds the initial document. It is intentionally *not* pushed back
///   on every keystroke (that would fight the cursor); instead `external_doc`
///   drives out-of-band content replacement (live agent authoring).
/// - `on_change` fires with the full document text on each edit.
#[component]
pub fn CodeEditor(
    value: String,
    language: EditorLanguage,
    /// Out-of-band content to force into the editor (e.g. an agent rewrite).
    /// When this changes, the editor content is replaced without disturbing the
    /// caret if the text already matches.
    external_doc: ReadSignal<Option<String>>,
    on_change: EventHandler<String>,
) -> Element {
    // A stable, unique DOM id for this editor's mount point. `use_hook` runs once
    // per component instance; the pointer address of a boxed marker gives us a
    // process-unique suffix without needing a RNG (which the sandbox forbids).
    let mount_id = use_hook(|| {
        let marker = Box::new(0u8);
        format!("rotero-editor-{:x}", Box::into_raw(marker) as usize)
    });

    // Mount the editor once the element is in the DOM. `value`/`language` are read
    // untracked here so edits don't remount; content updates go through
    // `external_doc`, language through the reconfigure effect below.
    {
        let mount_id = mount_id.clone();
        let initial = value.clone();
        let lang = language;
        use_effect(move || {
            let mount_id = mount_id.clone();
            let doc = js_string(&initial);
            let lang = lang.js_id();
            spawn(async move {
                // The element may not be painted on the first tick; retry briefly.
                let js = format!(
                    r#"(async function() {{
                        for (var i = 0; i < 40; i++) {{
                            if (window.__roteroEditor && document.getElementById("{mount_id}")) {{
                                window.__roteroEditor.mount("{mount_id}", {doc}, "{lang}");
                                return;
                            }}
                            await new Promise(r => setTimeout(r, 25));
                        }}
                    }})()"#
                );
                let _ = document::eval(&js);
            });
        });
    }

    // Reconfigure the language mode when it changes (no content loss).
    {
        let mount_id = mount_id.clone();
        use_effect(move || {
            let lang = language.js_id();
            let mount_id = mount_id.clone();
            spawn(async move {
                let _ = document::eval(&format!(
                    r#"window.__roteroEditor && window.__roteroEditor.setLanguage("{mount_id}", "{lang}")"#
                ));
            });
        });
    }

    // Push out-of-band content (agent authoring) into the editor. `setDoc` is a
    // no-op when the text already matches, so it won't clobber the user's caret.
    {
        let mount_id = mount_id.clone();
        use_effect(move || {
            if let Some(doc) = external_doc() {
                let mount_id = mount_id.clone();
                let doc = js_string(&doc);
                spawn(async move {
                    let _ = document::eval(&format!(
                        r#"window.__roteroEditor && window.__roteroEditor.setDoc("{mount_id}", {doc})"#
                    ));
                });
            }
        });
    }

    // Long-lived poll of the editor event queue: drain changes for *this* mount
    // id and forward them to `on_change`. Mirrors the graph view's bridge.
    {
        let mount_id = mount_id.clone();
        use_hook(move || {
            let mount_id = mount_id.clone();
            spawn(async move {
                let mut eval = document::eval(&format!(
                    r#"(async function() {{
                        while (true) {{
                            await new Promise(r => setTimeout(r, 120));
                            var q = window.__roteroEditorEvents || [];
                            if (q.length === 0) continue;
                            var mine = [];
                            var rest = [];
                            for (var i = 0; i < q.length; i++) {{
                                if (q[i].id === "{mount_id}") mine.push(q[i]);
                                else rest.push(q[i]);
                            }}
                            window.__roteroEditorEvents = rest;
                            // Only the latest change matters for a full-doc sync.
                            for (var j = mine.length - 1; j >= 0; j--) {{
                                if (mine[j].type === "change") {{
                                    dioxus.send(mine[j].value);
                                    break;
                                }}
                            }}
                        }}
                    }})()"#
                ));
                while let Ok(text) = eval.recv::<String>().await {
                    on_change.call(text);
                }
            });
        });
    }

    // Tear down the JS editor instance when this component unmounts.
    {
        let mount_id = mount_id.clone();
        use_drop(move || {
            let mount_id = mount_id.clone();
            spawn(async move {
                let _ = document::eval(&format!(
                    r#"window.__roteroEditor && window.__roteroEditor.unmount("{mount_id}")"#
                ));
            });
        });
    }

    rsx! {
        div { class: "code-editor-mount", id: "{mount_id}" }
    }
}
