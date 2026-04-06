use dioxus::prelude::*;

use crate::agent::types::ChatState;

/// Chat toggle button for the library header / PDF tab bar.
#[component]
pub fn ChatToggleButton() -> Element {
    let mut chat_state = use_context::<Signal<ChatState>>();
    let is_open = chat_state.read().panel_open;

    rsx! {
        button {
            class: "btn btn--ghost",
            class: if is_open { "chat-toggle-btn--active" } else { "" },
            title: "AI Chat",
            onclick: move |_| {
                chat_state.with_mut(|s| s.panel_open = !s.panel_open);
            },
            i { class: "bi bi-chat-dots" }
        }
    }
}
