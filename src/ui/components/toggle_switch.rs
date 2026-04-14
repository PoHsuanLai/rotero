use dioxus::prelude::*;

#[component]
pub fn ToggleSwitch(checked: bool, onchange: EventHandler<bool>) -> Element {
    rsx! {
        label { class: "settings-toggle",
            input {
                r#type: "checkbox",
                checked,
                onchange: move |evt| onchange.call(evt.checked()),
            }
            span { class: "settings-toggle-track",
                span { class: "settings-toggle-thumb" }
            }
        }
    }
}
