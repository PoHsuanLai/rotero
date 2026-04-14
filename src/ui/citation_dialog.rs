use dioxus::prelude::*;

use crate::state::app_state::LibraryState;
use crate::ui::components::modal::Modal;

#[component]
pub fn CitationDialog() -> Element {
    let lib_state = use_context::<Signal<LibraryState>>();
    let mut show = use_signal(|| false);
    let mut selected_style_idx = use_signal(|| 0usize);
    let mut formatted = use_signal(String::new);
    let mut error_msg = use_signal(|| None::<String>);
    let mut copied = use_signal(|| false);

    let state = lib_state.read();
    let selected_paper = state.selected_paper().cloned();
    drop(state);

    if !show() {
        return rsx! {
            button {
                class: "btn btn--ghost",
                disabled: selected_paper.is_none(),
                onclick: move |_| {
                    show.set(true);
                    let state = lib_state.read();
                    if let Some(paper) = state.selected_paper() {
                        let idx = selected_style_idx();
                        let (_, style) = rotero_bib::AVAILABLE_STYLES[idx];
                        match rotero_bib::format_citation(paper, style) {
                            Ok(text) => {
                                formatted.set(text);
                                error_msg.set(None);
                            }
                            Err(e) => error_msg.set(Some(e)),
                        }
                    }
                },
                "Cite"
            }
        };
    }

    let styles = rotero_bib::AVAILABLE_STYLES;

    rsx! {
        Modal {
            title: "Generate Citation".to_string(),
            on_close: move |_| show.set(false),
            width: "560px",

            div { class: "citation-style-picker",
                label { class: "detail-label", "Citation Style" }
                select {
                    class: "select citation-select",
                    value: "{selected_style_idx}",
                    onchange: move |evt| {
                        if let Ok(idx) = evt.value().parse::<usize>() {
                            selected_style_idx.set(idx);
                            let state = lib_state.read();
                            if let Some(paper) = state.selected_paper() {
                                let (_, style) = styles[idx];
                                match rotero_bib::format_citation(paper, style) {
                                    Ok(text) => {
                                        formatted.set(text);
                                        error_msg.set(None);
                                    }
                                    Err(e) => error_msg.set(Some(e)),
                                }
                            }
                        }
                    },
                    for (i, (name, _)) in styles.iter().enumerate() {
                        option { value: "{i}", "{name}" }
                    }
                }
            }

            div { class: "citation-preview",
                if let Some(ref err) = *error_msg.read() {
                    div { class: "error-message", "{err}" }
                } else {
                    pre { class: "citation-text", "{formatted}" }
                }
            }

            div { class: "citation-actions",
                button {
                    class: if copied() { "btn btn--copied" } else { "btn btn--primary" },
                    onclick: move |_| {
                        if let Ok(mut clip) = arboard::Clipboard::new() {
                            let _ = clip.set_text(formatted());
                            copied.set(true);
                            spawn(async move {
                                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                                copied.set(false);
                            });
                        }
                    },
                    if copied() { "Copied!" } else { "Copy to Clipboard" }
                }
            }
        }
    }
}
