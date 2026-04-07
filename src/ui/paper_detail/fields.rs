use dioxus::prelude::*;

use crate::state::app_state::LibraryState;
use rotero_db::Database;

#[component]
pub fn AddToCollectionSelect(paper_id: String) -> Element {
    let lib_state = use_context::<Signal<LibraryState>>();
    let db = use_context::<Database>();
    let collections = lib_state.read().collections.clone();

    rsx! {
        select {
            class: "select",
            onchange: {
                let paper_id = paper_id.clone();
                move |evt| {
                    let coll_id = evt.value();
                    if coll_id.is_empty() { return; }
                    let db = db.clone();
                    let pid = paper_id.clone();
                    spawn(async move {
                        let _ = rotero_db::collections::add_paper_to_collection(db.conn(), &pid, &coll_id).await;
                    });
                }
            },
            option { value: "", "Add to collection..." }
            for coll in collections.iter() {
                {
                    let cid = coll.id.clone().unwrap_or_default();
                    let cname = coll.name.clone();
                    rsx! { option { value: "{cid}", "{cname}" } }
                }
            }
        }
    }
}

#[component]
pub fn TagEditor(paper_id: String) -> Element {
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let db = use_context::<Database>();
    let mut new_tag = use_signal(String::new);

    rsx! {
        div { class: "tag-editor",
            input {
                id: "tag-editor-input",
                class: "input input--sm",
                r#type: "text",
                placeholder: "Add tag...",
                value: "{new_tag}",
                oninput: move |evt| new_tag.set(evt.value()),
                onkeypress: {
                    let paper_id = paper_id.clone();
                    move |evt| {
                    if evt.key() == Key::Enter {
                        let tag_name = new_tag().trim().to_string();
                        if tag_name.is_empty() { return; }
                        let db = db.clone();
                        let pid = paper_id.clone();
                        spawn(async move {
                            if let Ok(tag_id) = rotero_db::tags::get_or_create_tag(db.conn(), &tag_name, None).await {
                                let _ = rotero_db::tags::add_tag_to_paper(db.conn(), &pid, &tag_id).await;
                                if let Ok(tags) = rotero_db::tags::list_tags(db.conn()).await {
                                    lib_state.with_mut(|s| s.tags = tags);
                                }
                            }
                            new_tag.set(String::new());
                        });
                    }
                }},
            }
        }
    }
}
