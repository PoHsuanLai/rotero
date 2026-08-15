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
