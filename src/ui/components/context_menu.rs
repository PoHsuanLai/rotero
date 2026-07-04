use dioxus::prelude::*;

static CONTEXT_MENU_ID: &str = "rotero-context-menu";

#[component]
pub fn ContextMenu(x: f64, y: f64, on_close: EventHandler<()>, children: Element) -> Element {
    // Nudge the menu back inside the viewport if the click point is near an
    // edge. This is a progressive enhancement: the menu is already visible at
    // (x, y) from CSS, so it stays usable even if this JS never runs. Runs on
    // every (x, y) change so re-opening at a new spot re-clamps.
    use_effect(move || {
        let js = format!(
            r#"requestAnimationFrame(() => {{
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
            }})"#
        );
        spawn(async move {
            let _ = document::eval(&js).await;
        });
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
            style: "left: {x}px; top: {y}px;",
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
    // When `Some(false)`, the click does not bubble up to the ContextMenu's
    // auto-close handler. Use this for items whose `on_click` spawns async work
    // and closes the menu itself when done — otherwise the bubbling close
    // unmounts the menu component and cancels the in-flight spawned future.
    close_on_click: Option<bool>,
    on_click: EventHandler<()>,
) -> Element {
    let is_danger = danger.unwrap_or(false);
    let is_disabled = disabled.unwrap_or(false);
    let auto_close = close_on_click.unwrap_or(true);

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
            onclick: move |evt: Event<MouseData>| {
                if !auto_close {
                    evt.stop_propagation();
                }
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
