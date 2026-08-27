use dioxus::prelude::*;

use rotero_db::Database;

#[component]
pub fn NotesSection(paper_id: String) -> Element {
    let db = use_context::<Database>();

    // Keyed on the prop rather than an effect over a captured copy: the panel
    // reuses this component when the selection changes, so an effect that read
    // `paper_id` once would keep showing the first paper's notes under every
    // paper selected after it.
    let notes = use_resource({
        let db = db.clone();
        let pid = paper_id.clone();
        move || {
            let db = db.clone();
            let pid = pid.clone();
            async move { db.list_notes_for_paper(&pid).await.unwrap_or_default() }
        }
    });

    let note_list = notes.read();
    let note_list = match note_list.as_ref() {
        Some(notes) if !notes.is_empty() => notes,
        _ => return rsx! {},
    };

    rsx! {
        div { class: "detail-notes-section",
            label { class: "detail-label", "Notes ({note_list.len()})" }
            for note in note_list.iter() {
                {
                    let note_id = note.id.clone().unwrap_or_default();
                    let title = note.title.clone();
                    // Note bodies come from highlighted PDF text and from the
                    // agent, so ligatures, dashes, and non-Latin scripts are
                    // routine. A byte slice here panicked whenever one straddled
                    // the cut, blanking the detail panel.
                    let body_preview = rotero_models::truncate_chars(&note.body, 117);
                    let body_html = crate::ui::markdown::md_to_html(&body_preview);
                    let db_del = db.clone();
                    rsx! {
                        div { key: "note-{note_id}", class: "detail-note-card",
                            div { class: "detail-note-title", "{title}" }
                            div {
                                class: "detail-note-body rendered-latex",
                                dangerous_inner_html: "{body_html}",
                            }
                            button {
                                class: "btn--danger-sm",
                                onclick: move |_| {
                                    let db = db_del.clone();
                                    let nid = note_id.clone();
                                    let mut notes = notes;
                                    spawn(async move {
                                        let _ = db.delete_note(&nid).await;
                                        // Re-reads through the same query the
                                        // list was built from.
                                        notes.restart();
                                    });
                                },
                                "Delete"
                            }
                        }
                    }
                }
            }
        }
    }
}
