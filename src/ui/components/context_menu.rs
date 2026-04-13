use dioxus::prelude::*;

static CONTEXT_MENU_ID: &str = "rotero-context-menu";

#[component]
pub fn ContextMenu(x: f64, y: f64, on_close: EventHandler<()>, children: Element) -> Element {
    use_hook(move || {
        let js = format!(
            r#"setTimeout(() => {{
                let el = document.getElementById('{CONTEXT_MENU_ID}');
                if (!el) return;
                let rect = el.getBoundingClientRect();
                let vw = window.innerWidth;
                let vh = window.innerHeight;
                let x = {x};
                let y = {y};
                if (x + rect.width > vw) x = vw - rect.width - 4;
                if (y + rect.height > vh) y = vh - rect.height - 4;
                if (x < 0) x = 4;
                if (y < 0) y = 4;
                el.style.left = x + 'px';
                el.style.top = y + 'px';
                el.style.visibility = 'visible';
            }}, 0)"#
        );
        document::eval(&js);
    });

    rsx! {
        div {
            class: "context-menu-backdrop",
            onclick: move |_| on_close.call(()),
            oncontextmenu: move |evt| {
                evt.prevent_default();
                on_close.call(());
            },
        }
        div {
            id: CONTEXT_MENU_ID,
            class: "context-menu",
            style: "left: {x}px; top: {y}px; visibility: hidden;",
            onclick: move |_| on_close.call(()),
            {children}
        }
    }
}

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
            onclick: move |_| {
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

#[component]
pub fn ContextMenuSeparator() -> Element {
    rsx! {
        div { class: "context-menu-separator" }
    }
}
