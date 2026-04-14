pub mod collections;
pub mod context_menus;
pub mod nav;
pub mod open_pdf;
pub mod tags;

pub use nav::Sidebar;

use crate::state::app_state::{LibraryState, LibraryView};
use dioxus::prelude::*;

pub(super) type TagContextMenu = (String, String, Option<String>, f64, f64);

#[component]
pub(crate) fn SidebarItem(
    label: String,
    count: Option<usize>,
    icon: String,
    active: bool,
    view: LibraryView,
) -> Element {
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let class = if active {
        "sidebar-nav-item sidebar-nav-item--active"
    } else {
        "sidebar-nav-item"
    };

    let icon_class = match icon.as_str() {
        "doc" => "bi bi-journal-text",
        "clock" => "bi bi-clock",
        "star" => "bi bi-star",
        "circle" => "bi bi-circle",
        _ => "",
    };

    rsx! {
        div {
            class: "{class}",
            onclick: move |_| {
                lib_state.with_mut(|s| s.view = view.clone());
            },
            i { class: "sidebar-nav-icon {icon_class}" }
            span { class: "sidebar-nav-label", "{label}" }
            if let Some(n) = count {
                span { class: "sidebar-nav-count", "{n}" }
            }
        }
    }
}

#[component]
pub(crate) fn CollapsibleSection(
    title: String,
    initially_open: Option<bool>,
    action: Option<Element>,
    children: Element,
) -> Element {
    let mut open = use_signal(|| initially_open.unwrap_or(true));

    let arrow_class = if open() {
        "bi bi-chevron-down"
    } else {
        "bi bi-chevron-right"
    };

    rsx! {
        div { class: "sidebar-section",
            div { class: "sidebar-section-header",
                div {
                    class: "sidebar-section-toggle",
                    onclick: move |_| open.set(!open()),
                    i { class: "sidebar-section-arrow {arrow_class}" }
                    h3 { class: "sidebar-section-title", "{title}" }
                }
                if let Some(action_el) = action {
                    {action_el}
                }
            }
            if open() {
                div { class: "sidebar-section-content",
                    {children}
                }
            }
        }
    }
}
