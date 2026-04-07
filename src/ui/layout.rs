use dioxus::prelude::*;

use super::chat_panel::ChatPanel;
use super::graph_view::GraphView;
#[cfg(feature = "desktop")]
use super::keybindings::GlobalKeyHandler;
use super::library::LibraryPanel;
use super::paper_detail::PaperDetail;
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

    let dark = config.read().dark_mode;
    let scale = config.read().ui_scale.clone();
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

    // Gather context for the window-scoped keyboard handler
    #[cfg(feature = "desktop")]
    let onkeydown_handler = {
        use crate::app::{DevicePixelRatio, RenderChannel, ShowSettings};
        use crate::state::app_state::ViewerToolState;
        use crate::state::undo::UndoStack;
        use rotero_db::Database;

        let show_settings = use_context::<Signal<ShowSettings>>();
        let db = use_context::<Database>();
        let render_ch = use_context::<RenderChannel>();
        let new_coll_editing = use_context::<Signal<Option<Option<String>>>>();
        let undo_stack = use_context::<Signal<UndoStack>>();
        let tools = use_context::<Signal<ViewerToolState>>();
        let dpr_sig = use_context::<Signal<DevicePixelRatio>>();

        EventHandler::new(move |event: Event<KeyboardData>| {
            super::keybindings::handle_keydown(
                event,
                show_settings,
                lib_state,
                tab_mgr,
                db.clone(),
                render_ch,
                config,
                new_coll_editing,
                undo_stack,
                tools,
                dpr_sig,
            );
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
            Sidebar {
                collapsed: sidebar_collapsed(),
                on_toggle: move |_| sidebar_collapsed.toggle(),
            }
            div { class: "main-panel",
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
                            if lib_state.read().selected_paper_id.is_some() {
                                PaperDetail {}
                            }
                        }
                    },
                }
            }
            if chat_open {
                ChatPanel {}
            }
        }
    }
}
