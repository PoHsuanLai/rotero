//! Recovering which library papers a conversation touched.
//!
//! The agent names papers in two places: the `<rotero-context>` block the app
//! prepends to every prompt (read in `agent::helpers`, before the block is
//! stripped for display), and the results of the rotero MCP tools it calls.
//! This module reads the second.

use rotero_models::Paper;

/// Library paper ids appearing in an MCP tool result.
///
/// Tool results are pretty-printed JSON from the rotero MCP server, so a paper
/// arrives either as a serialized `Paper` (carrying `id`) or as a mutation
/// acknowledgement (carrying `paper_id`). Tags, collections, and notes also have
/// uuid `id` fields, so a bare `id` is only believed when its object also has the
/// `title` and `creators` that identify a `Paper` — which makes that struct's
/// serialized shape load-bearing here, hence the round-trip test below.
///
/// Ids are returned in the order found, deduplicated. Callers must still filter
/// against the library: the agent can name a paper that was never imported.
pub fn paper_ids_from_tool_output(output: &str) -> Vec<String> {
    let mut ids = Vec::new();
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(output) {
        walk(&value, &mut ids);
    }
    ids
}

fn walk(value: &serde_json::Value, ids: &mut Vec<String>) {
    match value {
        serde_json::Value::Object(map) => {
            if let Some(id) = map.get("paper_id").and_then(|v| v.as_str()) {
                push_unique(ids, id);
            }
            let looks_like_paper = map.contains_key("title") && map.contains_key("creators");
            if looks_like_paper && let Some(id) = map.get("id").and_then(|v| v.as_str()) {
                push_unique(ids, id);
            }
            for nested in map.values() {
                walk(nested, ids);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                walk(item, ids);
            }
        }
        _ => {}
    }
}

fn push_unique(ids: &mut Vec<String>, id: &str) {
    if !id.is_empty() && !ids.iter().any(|seen| seen == id) {
        ids.push(id.to_string());
    }
}

/// Title/authors/year extracted from the same MCP JSON `paper_ids_from_tool_output` walks.
///
/// Used by the chat tool-call chip so a search result can render as a paper
/// card without a second library round-trip. Ids are still best-effort: the
/// paper may not be in the library yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaperSnippet {
    pub id: Option<String>,
    pub title: String,
    pub authors: String,
    pub year: Option<i32>,
}

pub fn papers_from_tool_output(output: &str) -> Vec<PaperSnippet> {
    let mut papers = Vec::new();
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(output) {
        collect_papers(&value, &mut papers);
    }
    papers
}

fn collect_papers(value: &serde_json::Value, papers: &mut Vec<PaperSnippet>) {
    match value {
        serde_json::Value::Object(map) => {
            let looks_like_paper = map.contains_key("title") && map.contains_key("creators");
            if looks_like_paper && let Ok(paper) = serde_json::from_value::<Paper>(value.clone()) {
                let snippet = PaperSnippet {
                    id: paper.id.clone(),
                    authors: paper.formatted_authors(),
                    year: paper.year,
                    title: paper.title,
                };
                let seen = snippet
                    .id
                    .as_ref()
                    .is_some_and(|id| papers.iter().any(|p| p.id.as_deref() == Some(id)));
                if !seen {
                    papers.push(snippet);
                }
            }
            for nested in map.values() {
                collect_papers(nested, papers);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect_papers(item, papers);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paper(id: &str, title: &str) -> Paper {
        Paper {
            id: Some(id.to_string()),
            title: title.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn reads_ids_from_serialized_papers() {
        let json = serde_json::to_string_pretty(&vec![
            paper("11111111-1111-7111-8111-111111111111", "A"),
            paper("22222222-2222-7222-8222-222222222222", "B"),
        ])
        .unwrap();

        assert_eq!(
            paper_ids_from_tool_output(&json),
            vec![
                "11111111-1111-7111-8111-111111111111",
                "22222222-2222-7222-8222-222222222222"
            ]
        );
    }

    /// The discriminator depends on `Paper` serializing `title` and `creators`
    /// under those names. Renaming either silently stops paper capture, so the
    /// coupling is asserted rather than assumed.
    #[test]
    fn a_paper_serializes_the_fields_the_discriminator_relies_on() {
        let json = serde_json::to_value(paper("id-1", "A")).unwrap();
        let map = json.as_object().unwrap();
        assert!(map.contains_key("title"));
        assert!(map.contains_key("creators"));
        assert!(map.contains_key("id"));
    }

    #[test]
    fn reads_the_id_from_a_mutation_acknowledgement() {
        let json = r#"{"success": true, "paper_id": "33333333-3333-7333-8333-333333333333"}"#;
        assert_eq!(
            paper_ids_from_tool_output(json),
            vec!["33333333-3333-7333-8333-333333333333"]
        );
    }

    /// Tags, collections, and notes have uuid ids too. Treating those as papers
    /// would attach a conversation to whatever paper happened to share the id.
    #[test]
    fn ignores_ids_belonging_to_other_kinds_of_record() {
        let tags = r#"[{"id": "44444444-4444-7444-8444-444444444444", "name": "diffusion"}]"#;
        assert!(paper_ids_from_tool_output(tags).is_empty());

        let collections = r#"[{"id": "55555555-5555-7555-8555-555555555555", "name": "Reading"}]"#;
        assert!(paper_ids_from_tool_output(collections).is_empty());
    }

    #[test]
    fn finds_papers_nested_inside_an_envelope() {
        let json = format!(
            r#"{{"results": [{{"paper": {}}}], "total": 1}}"#,
            serde_json::to_string(&paper("66666666-6666-7666-8666-666666666666", "A")).unwrap()
        );
        assert_eq!(
            paper_ids_from_tool_output(&json),
            vec!["66666666-6666-7666-8666-666666666666"]
        );
    }

    #[test]
    fn a_repeated_paper_is_recorded_once() {
        let json = serde_json::to_string(&vec![
            paper("77777777-7777-7777-8777-777777777777", "A"),
            paper("77777777-7777-7777-8777-777777777777", "A"),
        ])
        .unwrap();
        assert_eq!(paper_ids_from_tool_output(&json).len(), 1);
    }

    /// Output can be truncated or be plain prose ("No paper found with ID ..."),
    /// which must read as no papers rather than panicking.
    #[test]
    fn unparseable_output_yields_nothing() {
        assert!(paper_ids_from_tool_output("").is_empty());
        assert!(paper_ids_from_tool_output("No paper found with ID abc").is_empty());
        assert!(paper_ids_from_tool_output(r#"[{"id": "trunc"#).is_empty());
    }

    #[test]
    fn papers_from_tool_output_keeps_title_authors_year() {
        let mut p = paper(
            "11111111-1111-7111-8111-111111111111",
            "Attention Is All You Need",
        );
        p.year = Some(2017);
        p.creators = vec![rotero_models::Creator::author("Ashish", "Vaswani")];
        let json = serde_json::to_string_pretty(&vec![p]).unwrap();
        let cards = papers_from_tool_output(&json);
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].title, "Attention Is All You Need");
        assert_eq!(cards[0].authors, "Ashish Vaswani");
        assert_eq!(cards[0].year, Some(2017));
    }
}
