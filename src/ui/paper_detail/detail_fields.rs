use dioxus::prelude::*;

use crate::ui::components::context_menu::{ContextMenu, ContextMenuItem};
use crate::ui::helpers::{item_type_icon, item_type_is_special, item_type_label, venue_label};
use rotero_models::{CreatorRole, Paper};

/// Non-author creators (editors, translators, …) grouped by role, so the detail
/// panel can show an "Editors"/"Translators" field beneath the authors. Each
/// entry is `(field label, comma-joined names)`, in first-seen role order.
fn contributor_groups(paper: &Paper) -> Vec<(&'static str, String)> {
    let mut order: Vec<&'static str> = Vec::new();
    let mut names: std::collections::HashMap<&'static str, Vec<String>> =
        std::collections::HashMap::new();

    for c in &paper.creators {
        let label = match c.role {
            CreatorRole::Author => continue,
            CreatorRole::Editor => "Editors",
            CreatorRole::SeriesEditor => "Series Editors",
            CreatorRole::Translator => "Translators",
            CreatorRole::Inventor => "Inventors",
            CreatorRole::Director => "Directors",
            CreatorRole::Contributor => "Contributors",
            CreatorRole::Other(_) => "Contributors",
        };
        let display = c.display_name();
        if display.is_empty() {
            continue;
        }
        names.entry(label).or_insert_with(|| {
            order.push(label);
            Vec::new()
        });
        names.get_mut(label).unwrap().push(display);
    }

    order
        .into_iter()
        .map(|label| (label, names.remove(label).unwrap_or_default().join(", ")))
        .collect()
}

/// The read-only bibliographic block shared by the library detail panel and the
/// web-result preview: item-type badge, title, authors, role-grouped
/// contributors, year, citations, venue, venue identifiers, DOI (with a copy /
/// open-in-browser context menu), and abstract.
///
/// Fields that are missing on the paper are simply skipped, so the same
/// component renders a fully-populated library record and a sparse web hit
/// without either side special-casing.
#[component]
pub fn DetailFields(paper: Paper) -> Element {
    let author_names = paper.author_names();
    let authors_display = if author_names.is_empty() {
        "Unknown".to_string()
    } else {
        author_names.join(", ")
    };

    let contributor_groups = contributor_groups(&paper);

    // Column-less venue identifiers, collapsed into one "Publication details"
    // block rather than a row each. Each entry is `(label, value)`.
    let venue_details: Vec<(&str, String)> = [
        ("Publisher", paper.publication.publisher.clone()),
        ("Place", paper.publication.place.clone()),
        ("Series", paper.publication.series.clone()),
        ("ISBN", paper.publication.isbn.clone()),
        ("ISSN", paper.publication.issn.clone()),
        ("Language", paper.publication.language.clone()),
    ]
    .into_iter()
    .filter_map(|(label, val)| val.filter(|v| !v.is_empty()).map(|v| (label, v)))
    .collect();

    let type_label = item_type_label(&paper.item_type);
    let type_icon = item_type_icon(&paper.item_type);
    let type_badge_class = if item_type_is_special(&paper.item_type) {
        "type-badge type-badge--special"
    } else {
        "type-badge"
    };

    let mut doi_ctx = use_signal(|| None::<(String, f64, f64)>);

    rsx! {
        div { class: "detail-field",
            div { class: "detail-title-row",
                label { class: "detail-label", "Title" }
                span { class: "{type_badge_class}", title: "{type_label}",
                    i { class: "bi {type_icon}" }
                    "{type_label}"
                }
            }
            div { class: "detail-value detail-value--title", "{paper.title}" }
        }

        div { class: "detail-field",
            label { class: "detail-label", "Authors" }
            div { class: "detail-value", "{authors_display}" }
        }

        for (role_label , role_names) in contributor_groups.iter() {
            div { class: "detail-field",
                label { class: "detail-label", "{role_label}" }
                div { class: "detail-value", "{role_names}" }
            }
        }

        if let Some(year) = paper.year {
            div { class: "detail-field",
                label { class: "detail-label", "Year" }
                div { class: "detail-value", "{year}" }
            }
        }

        if let Some(count) = paper.citation.citation_count {
            div { class: "detail-field",
                label { class: "detail-label", "Citations" }
                div { class: "detail-value detail-value--citations", "{count}" }
            }
        }

        if let Some(ref journal) = paper.publication.journal {
            div { class: "detail-field",
                label { class: "detail-label", "{venue_label(&paper.item_type)}" }
                div { class: "detail-value detail-value--journal", "{journal}" }
            }
        }

        if !venue_details.is_empty() {
            div { class: "detail-field",
                label { class: "detail-label", "Publication details" }
                dl { class: "detail-venue-grid",
                    for (label , value) in venue_details.iter() {
                        dt { "{label}" }
                        dd { "{value}" }
                    }
                }
            }
        }

        if let Some(ref doi) = paper.doi {
            {
                let doi_for_ctx = doi.clone();
                rsx! {
                    div { class: "detail-field",
                        label { class: "detail-label", "DOI" }
                        div {
                            class: "detail-value detail-value--doi",
                            oncontextmenu: move |evt: Event<MouseData>| {
                                evt.prevent_default();
                                doi_ctx.set(Some((doi_for_ctx.clone(), evt.client_coordinates().x, evt.client_coordinates().y)));
                            },
                            "{doi}"
                        }
                    }
                }
            }
        }

        if let Some(ref abstract_text) = paper.abstract_text {
            div { class: "detail-field",
                label { class: "detail-label", "Abstract" }
                div {
                    class: "detail-value detail-value--abstract rendered-latex",
                    dangerous_inner_html: "{crate::ui::markdown::text_with_latex(abstract_text)}",
                }
            }
        }

        if let Some((doi_str, mx, my)) = doi_ctx() {
            {
                let doi_copy = doi_str.clone();
                let doi_open = doi_str.clone();
                rsx! {
                    ContextMenu {
                        x: mx,
                        y: my,
                        on_close: move |_| doi_ctx.set(None),

                        ContextMenuItem {
                            label: "Copy DOI".to_string(),
                            icon: Some("bi-clipboard".to_string()),
                            on_click: move |_| {
                                if let Ok(mut clip) = arboard::Clipboard::new() {
                                    let _ = clip.set_text(&*doi_copy);
                                }
                                doi_ctx.set(None);
                            },
                        }

                        ContextMenuItem {
                            label: "Open in browser".to_string(),
                            icon: Some("bi-box-arrow-up-right".to_string()),
                            on_click: move |_| {
                                // Route to the right resolver: doi.org rejects
                                // arXiv's `arXiv:ID` pseudo-DOI, so parse first.
                                let url = rotero_models::PaperId::parse(&doi_open)
                                    .map(|pid| pid.resolve_url())
                                    .unwrap_or_else(|| format!("https://doi.org/{doi_open}"));
                                let _ = open::that(&url);
                                doi_ctx.set(None);
                            },
                        }
                    }
                }
            }
        }
    }
}
