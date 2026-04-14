use dioxus::prelude::*;

#[component]
pub fn SettingsSelect(
    value: String,
    options: Vec<(String, String)>,
    onchange: EventHandler<String>,
) -> Element {
    rsx! {
        select {
            class: "select settings-select",
            value: "{value}",
            onchange: move |evt| onchange.call(evt.value()),
            for (val, label) in options.iter() {
                option { value: "{val}", "{label}" }
            }
        }
    }
}
