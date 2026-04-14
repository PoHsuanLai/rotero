use dioxus::prelude::*;

#[component]
pub fn SettingsField(label: &'static str, children: Element) -> Element {
    rsx! {
        div { class: "settings-field",
            span { class: "settings-field-label", "{label}" }
            div { class: "settings-field-control", {children} }
        }
    }
}
