use dioxus::prelude::*;

/// A positioned context menu that appears on right-click.
#[component]
pub fn ContextMenu(
    x: f64,
    y: f64,
    on_close: EventHandler<()>,
    children: Element,
) -> Element {
    rsx! {
        // Invisible backdrop to catch clicks outside
        div {
            class: "context-menu-backdrop",
            onclick: move |_| on_close.call(()),
            oncontextmenu: move |evt| {
                evt.prevent_default();
                on_close.call(());
            },
        }
        div {
            class: "context-menu",
            style: "left: {x}px; top: {y}px;",
            onclick: move |_| on_close.call(()),
            {children}
        }
    }
}

/// A single item in the context menu.
#[component]
pub fn ContextMenuItem(
    label: String,
    icon: Option<String>,
    danger: Option<bool>,
    disabled: Option<bool>,
    on_click: EventHandler<()>,
) -> Element {
    let is_danger = danger.unwrap_or(false);
    let is_disabled = disabled.unwrap_or(false);

    let mut class = String::from("context-menu-item");
    if is_danger {
        class.push_str(" context-menu-item--danger");
    }
    if is_disabled {
        class.push_str(" context-menu-item--disabled");
    }

    rsx! {
        div {
            class: "{class}",
            onclick: move |evt| {
                evt.stop_propagation();
                if !is_disabled {
                    on_click.call(());
                }
            },
            if let Some(ref icon_class) = icon {
                i { class: "context-menu-icon bi {icon_class}" }
            }
            span { class: "context-menu-label", "{label}" }
        }
    }
}

/// A separator line in the context menu.
#[component]
pub fn ContextMenuSeparator() -> Element {
    rsx! {
        div { class: "context-menu-separator" }
    }
}
