use dioxus::prelude::*;

#[component]
pub fn PathField(
    label: &'static str,
    path: String,
    show_reset: bool,
    on_pick: EventHandler<()>,
    on_clear: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "settings-field",
            span { class: "settings-field-label", "{label}" }
            div { class: "settings-field-control settings-path-control",
                code { class: "settings-bib-path", "{path}" }
                button {
                    class: "btn btn--sm btn--secondary",
                    onclick: move |_| on_pick.call(()),
                    "Change..."
                }
                if show_reset {
                    button {
                        class: "btn btn--sm btn--ghost",
                        onclick: move |_| on_clear.call(()),
                        "Reset"
                    }
                }
            }
        }
    }
}
