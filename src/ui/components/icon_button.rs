use dioxus::prelude::*;

#[component]
pub fn IconButton(
    icon: String,
    tooltip: String,
    active: Option<bool>,
    onclick: EventHandler<()>,
) -> Element {
    let is_active = active.unwrap_or(false);

    rsx! {
        button {
            class: "icon-btn",
            class: if is_active { "icon-btn--active" } else { "" },
            onclick: move |_| onclick.call(()),
            i { class: "bi bi-{icon}" }
            span { class: "icon-btn-tooltip", "{tooltip}" }
        }
    }
}
