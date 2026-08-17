//! Transient messages for failures that happen while the app is running.
//!
//! Distinct from [`PreflightBanner`](super::preflight_banner::PreflightBanner),
//! which reports what was already wrong at startup. This is for the rest: a
//! settings write that could not reach disk, a tag that failed to save. Before
//! it existed there was no way to say any of that, which is why those writes
//! were discarded with `let _ =` — a silent failure at least left the UI
//! consistent with itself, whereas reporting one meant a button that did
//! nothing with no explanation.

use dioxus::prelude::*;

use crate::state::app_state::LibraryState;

/// How long a confirmation stays up. Errors are not auto-dismissed: they are
/// the ones the user needs to actually read.
const INFO_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(4);

#[component]
pub fn Toasts() -> Element {
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let toasts = lib_state.read().toasts.clone();

    // Expire confirmations. Errors stay until dismissed.
    use_future(move || async move {
        loop {
            tokio::time::sleep(INFO_TIMEOUT).await;
            let expired: Vec<u64> = lib_state
                .peek()
                .toasts
                .iter()
                .filter(|t| !t.is_error)
                .map(|t| t.id)
                .collect();
            if !expired.is_empty() {
                lib_state.with_mut(|s| {
                    for id in expired {
                        s.dismiss_toast(id);
                    }
                });
            }
        }
    });

    if toasts.is_empty() {
        return rsx! {};
    }

    rsx! {
        div { class: "toast-stack", role: "status", aria_live: "polite",
            for toast in toasts {
                div {
                    key: "toast-{toast.id}",
                    class: if toast.is_error { "toast toast-error" } else { "toast" },
                    span { class: "toast-message", "{toast.message}" }
                    button {
                        class: "toast-dismiss",
                        aria_label: "Dismiss",
                        onclick: move |_| lib_state.with_mut(|s| s.dismiss_toast(toast.id)),
                        "×"
                    }
                }
            }
        }
    }
}
