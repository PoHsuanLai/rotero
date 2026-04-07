use dioxus::prelude::*;

use crate::agent::types::ChatState;

#[component]
pub fn ChatToggleButton() -> Element {
    let mut chat_state = use_context::<Signal<ChatState>>();
    let is_open = chat_state.read().panel_open;

    let class = if is_open {
        "btn btn--ghost-active btn--sm"
    } else {
        "btn btn--ghost btn--sm"
    };

    rsx! {
        button {
            class,
            onclick: move |_| {
                chat_state.with_mut(|s| s.panel_open = !s.panel_open);
            },
            i { class: "bi bi-chat-dots" }
            " Chat"
        }
    }
}
