use dioxus::prelude::*;

use crate::state::app_state::LibraryState;

#[component]
pub fn CitationDialog() -> Element {
    let lib_state = use_context::<Signal<LibraryState>>();
    let mut show = use_signal(|| false);
    let mut selected_style_idx = use_signal(|| 0usize);
    let mut formatted = use_signal(|| String::new());
    let mut error_msg = use_signal(|| None::<String>);

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
                    // Generate citation on open
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
        div { class: "citation-overlay",
            onclick: move |_| show.set(false),

            div { class: "citation-dialog",
                onclick: move |evt| evt.stop_propagation(),

                div { class: "citation-header",
                    h3 { "Generate Citation" }
                    button {
                        class: "detail-close",
                        onclick: move |_| show.set(false),
                        "\u{00d7}"
                    }
                }

                // Style picker
                div { class: "citation-style-picker",
                    label { class: "detail-label", "Citation Style" }
                    select {
                        class: "select citation-select",
                        value: "{selected_style_idx}",
                        onchange: move |evt| {
                            if let Ok(idx) = evt.value().parse::<usize>() {
                                selected_style_idx.set(idx);
                                // Regenerate
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

                // Preview
                div { class: "citation-preview",
                    if let Some(ref err) = *error_msg.read() {
                        div { class: "error-message", "{err}" }
                    } else {
                        pre { class: "citation-text", "{formatted}" }
                    }
                }

                // Copy button
                div { class: "citation-actions",
                    button {
                        class: "btn btn--primary",
                        onclick: move |_| {
                            // Copy to clipboard via eval
                            let text = formatted().replace('`', "\\`");
                            let js = format!("navigator.clipboard.writeText(`{text}`)");
                            document::eval(&js);
                        },
                        "Copy to Clipboard"
                    }

                    // Generate bibliography for all papers
                    button {
                        class: "btn btn--ghost",
                        onclick: move |_| {
                            let papers = lib_state.read().papers.clone();
                            let idx = selected_style_idx();
                            let (_, style) = styles[idx];
                            match rotero_bib::format_bibliography(&papers, style) {
                                Ok(text) => {
                                    formatted.set(text);
                                    error_msg.set(None);
                                }
                                Err(e) => error_msg.set(Some(e)),
                            }
                        },
                        "All Papers Bibliography"
                    }
                }
            }
        }
    }
}
