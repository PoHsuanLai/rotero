use dioxus::prelude::*;

use crate::agent::types::ChatState;
use crate::ui::components::icon_button::IconButton;

/// Chat toggle button for the library header / PDF tab bar.
#[component]
pub fn ChatToggleButton() -> Element {
    let mut chat_state = use_context::<Signal<ChatState>>();
    let is_open = chat_state.read().panel_open;

    rsx! {
        IconButton {
            icon: "chat-dots",
            tooltip: "AI Chat",
            active: is_open,
            onclick: move |_| {
                chat_state.with_mut(|s| s.panel_open = !s.panel_open);
            },
        }
    }
}
