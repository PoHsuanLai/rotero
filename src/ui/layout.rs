use dioxus::prelude::*;

use super::chat_panel::ChatPanel;
use super::components::preflight_banner::PreflightBanner;
use super::components::toasts::Toasts;
use super::graph_view::GraphView;
#[cfg(feature = "desktop")]
use super::keybindings::GlobalKeyHandler;
use super::library::LibraryPanel;
use super::paper_detail::{MultiSelectSummary, PaperDetail, WebPreview};
use super::pdf::{PdfTabBar, PdfViewer};
use super::sidebar::Sidebar;
use crate::agent::types::ChatState;
use crate::state::app_state::{LibraryState, LibraryView, PdfTabManager};
use crate::sync::engine::SyncConfig;

#[component]
pub fn Layout() -> Element {
    let lib_state = use_context::<Signal<LibraryState>>();
    let tab_mgr = use_context::<Signal<PdfTabManager>>();
    let config = use_context::<Signal<SyncConfig>>();
    let chat_state = use_context::<Signal<ChatState>>();
    let mut sidebar_collapsed = use_signal(|| false);
    let view = lib_state.read().view.clone();
    let chat_open = chat_state.read().panel_open;

    let dark = config.read().ui.dark_mode;
    let scale = config.read().ui.ui_scale.clone();
    let has_tabs = tab_mgr.read().active_tab_id.is_some();

    let container_class = if dark {
        "app-container dark"
    } else {
        "app-container"
    };

    #[cfg(feature = "desktop")]
    let key_handler = rsx! { GlobalKeyHandler {} };
    #[cfg(not(feature = "desktop"))]
    let key_handler = rsx! {};

    #[cfg(feature = "desktop")]
    let onkeydown_handler = {
        let (ctx, db) = super::keybindings::KeyCtx::from_context();

        EventHandler::new(move |event: Event<KeyboardData>| {
            super::keybindings::handle_keydown(event, ctx, db.clone());
        })
    };
    #[cfg(not(feature = "desktop"))]
    let onkeydown_handler = EventHandler::new(move |_: Event<KeyboardData>| {});

    rsx! {
        {key_handler}
        div {
            class: "{container_class}",
            "data-scale": "{scale}",
            tabindex: "0",
            onkeydown: onkeydown_handler,
            oncontextmenu: move |evt| evt.prevent_default(),
            Sidebar {
                collapsed: sidebar_collapsed(),
                on_toggle: move |_| sidebar_collapsed.toggle(),
            }
            div { class: "main-panel",
                PreflightBanner {}
                Toasts {}
                match view {
                    LibraryView::PdfViewer if has_tabs => rsx! {
                        PdfTabBar {}
                        PdfViewer {}
                    },
                    LibraryView::Graph => rsx! {
                        GraphView {}
                    },
                    _ => rsx! {
                        div { style: "flex: 1; display: flex; min-height: 0;",
                            LibraryPanel {}
                            {
                                let state = lib_state.read();
                                if state.previewed_web.is_some() {
                                    rsx! { WebPreview {} }
                                } else {
                                    match state.selection_count() {
                                        0 => rsx! {},
                                        1 => rsx! { PaperDetail {} },
                                        _ => rsx! { MultiSelectSummary {} },
                                    }
                                }
                            }
                        }
                    },
                }
            }
            if chat_open {
                ChatPanel {}
            }
        }
        super::import_export::OaOverlay {}
        {update_dialog_element()}
    }
}

#[cfg(feature = "desktop")]
fn update_dialog_element() -> Element {
    rsx! { super::update_dialog::UpdateDialog {} }
}

#[cfg(not(feature = "desktop"))]
fn update_dialog_element() -> Element {
    rsx! {}
}
