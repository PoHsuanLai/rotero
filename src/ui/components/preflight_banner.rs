//! Surfaces startup failures that would otherwise only reach the log file.
//!
//! A broken connector port or a library missing its sync metadata leaves the app
//! looking fine while silently doing nothing, which is how a bad install went
//! unnoticed on machines other than the developer's. One dismissible strip is
//! enough: it names the subsystem and the reason, and stays out of the way once
//! the user has read it.

use dioxus::prelude::*;

/// A dismissible strip listing whatever failed at startup. Renders nothing when
/// startup was clean, which is the overwhelmingly common case.
#[component]
pub fn PreflightBanner() -> Element {
    #[cfg(feature = "desktop")]
    {
        let mut dismissed = use_signal(|| false);
        // Most of these are decided at startup, but sync runs on a timer and can
        // start failing at any point, so poll rather than snapshotting once.
        let mut preflight = use_signal(crate::init::preflight::snapshot);
        use_future(move || async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                let latest = crate::init::preflight::snapshot();
                if latest != *preflight.peek() {
                    // A newly-reported problem is worth showing again, even if
                    // the user dismissed an earlier one.
                    dismissed.set(false);
                    preflight.set(latest);
                }
            }
        });

        let preflight = preflight();
        if preflight.is_healthy() || dismissed() {
            return rsx! {};
        }

        let issues = preflight.issues();
        rsx! {
            div { class: "preflight-banner", role: "status",
                div { class: "preflight-banner-body",
                    for (subsystem, message) in issues {
                        div { class: "preflight-banner-issue",
                            span { class: "preflight-banner-subsystem", "{subsystem}" }
                            span { "{message}" }
                        }
                    }
                }
                button {
                    class: "preflight-banner-dismiss",
                    aria_label: "Dismiss",
                    onclick: move |_| dismissed.set(true),
                    "×"
                }
            }
        }
    }

    #[cfg(not(feature = "desktop"))]
    rsx! {}
}
