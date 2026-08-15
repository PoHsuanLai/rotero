//! Drives the UI into the states the documentation screenshots capture.
//!
//! The capture script writes a step file and sets `ROTERO_SHOT_SCRIPT` to its
//! path; this component runs the steps and rewrites the file to `done` so the
//! script knows when to fire `screencapture`.
//!
//! Deliberately constrained:
//!
//! - `debug_assertions` only, so it cannot exist in a release build.
//! - Inert unless `ROTERO_SHOT_SCRIPT` is set, so a normal `dx serve` session
//!   never mounts it.
//! - Reads from a file rather than a socket. Evaluating attacker-supplied
//!   JavaScript in the app's WebView would be remote code execution, and a
//!   local file the developer already wrote is not a new capability.

use dioxus::prelude::*;

/// Polls the step file and applies each step in order.
#[component]
pub fn ShotDriver() -> Element {
    use_hook(|| {
        let Ok(path) = std::env::var("ROTERO_SHOT_SCRIPT") else {
            return;
        };

        spawn(async move {
            // Most of the app's controls are `button.btn` distinguished only by
            // their label, so steps select by text. Defined once here rather
            // than repeated inside every step in the manifest.
            // A raw string, not a `\`-continued one: continuations fold the
            // source onto a single line, which would put everything after the
            // first `//` inside a comment.
            let _ = document::eval(
                r#"
                window.byText = (sel, text) =>
                    [...document.querySelectorAll(sel)]
                        .find(el => el.textContent.trim().includes(text));

                // `el.click()` produces an event with no trigger button, and
                // handlers that require a primary click — paper selection runs
                // off onmouseup — ignore it. `buttons` is the mask of buttons
                // still held (1 while down, 0 once released); `button` stays 0
                // for primary throughout, which is what Dioxus reads.
                window.realClick = (el) => {
                    if (!el) return false;
                    const send = (type, buttons) => el.dispatchEvent(
                        new MouseEvent(type, {
                            bubbles: true, cancelable: true, view: window,
                            button: 0, buttons
                        })
                    );
                    send('mousedown', 1);
                    send('mouseup', 0);
                    send('click', 0);
                    return true;
                };
                "#,
            );

            loop {
                sleep_ms(250).await;

                let Ok(body) = std::fs::read_to_string(&path) else {
                    continue;
                };
                let body = body.trim();
                if body.is_empty() || body == "done" {
                    continue;
                }

                for line in body.lines() {
                    let Some((verb, arg)) = line.split_once(char::is_whitespace) else {
                        continue;
                    };
                    match verb.trim() {
                        "click" => {
                            let sel = escape(arg.trim());
                            let _ = document::eval(&format!(
                                "document.querySelector(`{sel}`)?.click()"
                            ));
                        }
                        "type" => {
                            if let Some((sel, text)) = arg.trim().split_once('|') {
                                let sel = escape(sel);
                                let text = escape(text);
                                // Dioxus listens for `input`, so setting `value`
                                // alone would update the DOM but not the state.
                                let _ = document::eval(&format!(
                                    "(() => {{ const el = document.querySelector(`{sel}`); \
                                     if (!el) return; el.focus(); el.value = `{text}`; \
                                     el.dispatchEvent(new Event('input', {{ bubbles: true }})); }})()"
                                ));
                            }
                        }
                        "eval" => {
                            let _ = document::eval(arg.trim());
                        }
                        "wait" => {
                            let ms = arg.trim().parse().unwrap_or(500);
                            sleep_ms(ms).await;
                        }
                        _ => {}
                    }
                }

                let _ = std::fs::write(&path, "done");
            }
        });
    });

    rsx! {}
}

/// Backtick and `${` would end the template literal the step is interpolated
/// into; a selector containing either is a bug in the manifest, not an attack.
fn escape(raw: &str) -> String {
    raw.replace('\\', "\\\\")
        .replace('`', "\\`")
        .replace("${", "\\${")
}

async fn sleep_ms(ms: u64) {
    tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
}
