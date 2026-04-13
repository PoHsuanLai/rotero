use dioxus::prelude::*;

use rotero_db::Database;

#[component]
pub fn NotesSection(paper_id: String) -> Element {
    let db = use_context::<Database>();
    let mut notes = use_signal(Vec::new);

    {
        let db = db.clone();
        let pid = paper_id.clone();
        use_effect(move || {
            let db = db.clone();
            let pid = pid.clone();
            spawn(async move {
                if let Ok(paper_notes) =
                    rotero_db::notes::list_notes_for_paper(db.conn(), &pid).await
                {
                    notes.set(paper_notes);
                }
            });
        });
    }

    let note_list = notes.read();
    if note_list.is_empty() {
        return rsx! {};
    }

    rsx! {
        div { class: "detail-notes-section",
            label { class: "detail-label", "Notes ({note_list.len()})" }
            for note in note_list.iter() {
                {
                    let note_id = note.id.clone().unwrap_or_default();
                    let title = note.title.clone();
                    let body_preview = if note.body.len() > 120 {
                        format!("{}...", &note.body[..117])
                    } else {
                        note.body.clone()
                    };
                    let body_html = crate::ui::markdown::md_to_html(&body_preview);
                    let db_del = db.clone();
                    let pid = paper_id.clone();
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
                                    let pid = pid.clone();
                                    spawn(async move {
                                        let _ = rotero_db::notes::delete_note(db.conn(), &nid).await;
                                        if let Ok(paper_notes) = rotero_db::notes::list_notes_for_paper(db.conn(), &pid).await {
                                            notes.set(paper_notes);
                                        }
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
