//! Claims filed against a paper, and the concept page one of them is about.
//!
//! The concept view replaces the claim list in this panel. It is a signal, not
//! a route: the library selection stays the paper.

use dioxus::prelude::*;

use rotero_db::Database;
use rotero_models::{Claim, Concept, ConceptPage};

#[component]
pub fn WikiSection(paper_id: String) -> Element {
    let db = use_context::<Database>();
    let mut open_concept = use_signal(|| None::<String>);
    let mut bound_paper = use_signal(String::new);

    // The detail panel reuses this component across selections. Dropping the
    // open concept when the paper changes keeps the next paper from opening
    // on the previous one's page.
    if bound_paper.read().as_str() != paper_id {
        bound_paper.set(paper_id.clone());
        open_concept.set(None);
    }

    // These hooks stay above the concept-view return. Opening a concept and
    // coming back must not change how many hooks this component runs.
    let db_load = db.clone();
    let claims = use_resource(use_reactive!(|paper_id| {
        let db = db_load.clone();
        async move { load_claims(&db, &paper_id).await }
    }));
    let chat_state = use_context::<Signal<crate::agent::types::ChatState>>();
    let idle = matches!(
        chat_state.read().status,
        crate::agent::types::AgentStatus::Idle
    );

    let concept_id = open_concept.read().clone();
    if let Some(concept_id) = concept_id {
        return rsx! {
            ConceptView {
                concept_id,
                on_back: move |_| open_concept.set(None),
                on_open: move |id: String| open_concept.set(Some(id)),
            }
        };
    }

    rsx! {
        div { class: "detail-notes-section",
            div { class: "detail-wiki-head",
                label { class: "detail-label", "Wiki" }
                CompileButton { paper_id: paper_id.clone(), idle }
            }
            match claims.read().as_ref() {
                Some(Ok(rows)) if !rows.is_empty() => rsx! {
                    for (claim, concepts) in rows.iter() {
                        ClaimCard {
                            key: "{claim.id.clone().unwrap_or_default()}",
                            claim: claim.clone(),
                            concepts: concepts.clone(),
                            on_open: move |id: String| open_concept.set(Some(id)),
                        }
                    }
                },
                Some(Err(message)) => rsx! {
                    p { class: "detail-claim-quote", "{message}" }
                },
                _ => rsx! {},
            }
        }
    }
}

#[component]
fn CompileButton(paper_id: String, idle: bool) -> Element {
    let db = use_context::<Database>();
    let agent_channel = use_context::<crate::ui::chat_panel::AgentChannel>();
    let mut chat_state = use_context::<Signal<crate::agent::types::ChatState>>();

    rsx! {
        button {
            class: "btn btn--secondary btn--sm",
            disabled: !idle,
            title: "Ask the agent to file sourced claims for this paper. Your notes stay as they are.",
            onclick: move |_| {
                let db = db.clone();
                let paper_id = paper_id.clone();
                spawn(async move {
                    let title = db
                        .get_paper_by_id(&paper_id)
                        .await
                        .ok()
                        .flatten()
                        .map(|p| p.title)
                        .unwrap_or_default();
                    let prompt = rotero_models::compile_paper_prompt(&paper_id, &title);
                    let paper_context = Some(format!(
                        "<rotero-context>\nPaper ID: {paper_id}\nTitle: {title}\n</rotero-context>"
                    ));
                    chat_state.with_mut(|s| {
                        s.panel_open = true;
                        s.messages.push(crate::agent::types::ChatMessage::hidden(
                            crate::agent::types::ChatRole::User,
                            vec![crate::agent::types::MessageContent::Text(prompt.clone())],
                        ));
                        s.status = crate::agent::types::AgentStatus::Streaming;
                    });
                    agent_channel.send(crate::agent::types::ChatRequest::SendMessage {
                        prompt,
                        paper_context,
                    });
                });
            },
            "Add to wiki"
        }
    }
}

#[component]
fn ClaimCard(claim: Claim, concepts: Vec<Concept>, on_open: EventHandler<String>) -> Element {
    let page = claim.page.map(|p| format!("p. {p}")).unwrap_or_default();
    let status = match claim.status {
        rotero_models::ClaimStatus::Confirmed => "Confirmed",
        rotero_models::ClaimStatus::Extracted => "Extracted",
        rotero_models::ClaimStatus::FromChat => "From chat",
    };
    let quote = rotero_models::truncate_chars(&claim.quote, 180);

    rsx! {
        div { class: "detail-note-card",
            div { class: "detail-note-title", "{claim.statement}" }
            if !quote.is_empty() {
                div { class: "detail-claim-quote", "“{quote}”" }
            }
            div { class: "detail-wiki-meta",
                span { "{status}" }
                if !page.is_empty() {
                    span { "{page}" }
                }
            }
            if !concepts.is_empty() {
                div { class: "detail-wiki-concepts",
                    for concept in concepts.iter() {
                        {
                            let id = concept.id.clone().unwrap_or_default();
                            let title = concept.title.clone();
                            rsx! {
                                button {
                                    key: "{id}",
                                    class: "btn btn--ghost btn--sm",
                                    onclick: move |_| on_open.call(id.clone()),
                                    "{title}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ConceptView(
    concept_id: String,
    on_back: EventHandler<()>,
    on_open: EventHandler<String>,
) -> Element {
    let db = use_context::<Database>();
    let db_load = db.clone();
    let page = use_resource(use_reactive!(|concept_id| {
        let db = db_load.clone();
        async move { db.read_concept(&concept_id).await.ok() }
    }));

    let loaded = page.read();
    let Some(Some(page)) = loaded.as_ref() else {
        return rsx! {
            div { class: "detail-notes-section",
                button {
                    class: "btn btn--ghost btn--sm",
                    onclick: move |_| on_back.call(()),
                    "Back to paper"
                }
            }
        };
    };

    let body_html = crate::ui::markdown::md_to_html(&page.concept.body);
    let kind = page.concept.kind.as_str();

    rsx! {
        div { class: "detail-notes-section",
            button {
                class: "btn btn--ghost btn--sm",
                onclick: move |_| on_back.call(()),
                "Back to paper"
            }
            div { class: "detail-note-title", "{page.concept.title}" }
            div { class: "detail-wiki-meta",
                span { "{kind}" }
            }
            if !page.concept.body.is_empty() {
                div {
                    class: "detail-note-body rendered-latex",
                    dangerous_inner_html: "{body_html}",
                }
            }
            RelatedConcepts { page: page.clone(), on_open }
            if !page.claims.is_empty() {
                label { class: "detail-label", "Claims" }
                for claim in page.claims.iter() {
                    div { key: "{claim.id.clone().unwrap_or_default()}", class: "detail-note-card",
                        div { class: "detail-note-title", "{claim.statement}" }
                        if !claim.quote.is_empty() {
                            div { class: "detail-claim-quote", "“{claim.quote}”" }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn RelatedConcepts(page: ConceptPage, on_open: EventHandler<String>) -> Element {
    if page.related.is_empty() {
        return rsx! {};
    }
    rsx! {
        div { class: "detail-wiki-concepts",
            for concept in page.related.iter() {
                {
                    let id = concept.id.clone().unwrap_or_default();
                    let title = concept.title.clone();
                    rsx! {
                        button {
                            key: "{id}",
                            class: "btn btn--ghost btn--sm",
                            onclick: move |_| on_open.call(id.clone()),
                            "{title}"
                        }
                    }
                }
            }
        }
    }
}

async fn load_claims(db: &Database, paper_id: &str) -> Result<Vec<(Claim, Vec<Concept>)>, String> {
    let claims = db
        .list_claims_for_paper(paper_id)
        .await
        .map_err(|e| e.to_string())?;
    let mut rows = Vec::with_capacity(claims.len());
    for claim in claims {
        let id = claim.id.clone().unwrap_or_default();
        let concepts = db
            .concepts_for_claim(&id)
            .await
            .map_err(|e| e.to_string())?;
        rows.push((claim, concepts));
    }
    Ok(rows)
}
