//! Tool implementation functions and ServerHandler implementation.

use rmcp::{
    RoleServer, ServerHandler,
    handler::server::wrapper::Parameters,
    model::{
        AnnotateAble, CallToolResult, Content, GetPromptRequestParams, GetPromptResult,
        ListPromptsResult, ListResourcesResult, PaginatedRequestParams, Prompt, PromptMessage,
        PromptMessageRole, ReadResourceRequestParams, ReadResourceResult, ResourceContents,
    },
    service::RequestContext,
    tool, tool_handler,
};
use serde::Serialize;

use super::RoteroMcp;
use super::params::*;

pub(super) fn err(msg: impl std::fmt::Display) -> rmcp::ErrorData {
    rmcp::ErrorData::internal_error(msg.to_string(), None)
}

pub(super) fn json_result<T: Serialize>(value: &T) -> Result<CallToolResult, rmcp::ErrorData> {
    let text = serde_json::to_string_pretty(value).map_err(|e| err(e))?;
    Ok(CallToolResult::success(vec![Content::text(text)]))
}

// -- Tool implementations ----------------------------------------------------

#[rmcp::tool_router]
impl RoteroMcp {
    #[tool(description = "Search papers in the library by title, authors, abstract, or full text")]
    async fn search_papers(
        &self,
        Parameters(params): Parameters<SearchPapersParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let mut papers = self
            .db
            .search_papers(&params.query)
            .await
            .map_err(|e| err(e))?;
        let limit = params.limit.unwrap_or(20).min(50) as usize;
        papers.truncate(limit);
        json_result(&papers)
    }

    #[tool(description = "Get full metadata for a paper by its ID")]
    async fn get_paper(
        &self,
        Parameters(params): Parameters<GetPaperParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let paper = self
            .db
            .get_paper_by_id(&params.paper_id)
            .await
            .map_err(|e| err(e))?;
        match paper {
            Some(p) => json_result(&p),
            None => Ok(CallToolResult::success(vec![Content::text(format!(
                "No paper found with ID {}",
                params.paper_id
            ))])),
        }
    }

    #[tool(description = "List papers in the library with pagination")]
    async fn list_papers(
        &self,
        Parameters(params): Parameters<ListPapersParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let offset = params.offset.unwrap_or(0);
        let limit = params.limit.unwrap_or(20).min(100);
        let total = self.db.count_papers().await.map_err(|e| err(e))?;
        let papers = self
            .db
            .list_papers(offset, limit)
            .await
            .map_err(|e| err(e))?;

        #[derive(Serialize)]
        struct ListResult {
            papers: Vec<rotero_models::Paper>,
            total: u32,
            offset: u32,
            limit: u32,
        }
        json_result(&ListResult {
            papers,
            total,
            offset,
            limit,
        })
    }

    #[tool(description = "Get all annotations (highlights, notes, underlines) for a paper")]
    async fn get_paper_annotations(
        &self,
        Parameters(params): Parameters<PaperIdParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let anns = self
            .db
            .list_annotations_for_paper(&params.paper_id)
            .await
            .map_err(|e| err(e))?;
        json_result(&anns)
    }

    #[tool(description = "Get all notes for a paper")]
    async fn get_paper_notes(
        &self,
        Parameters(params): Parameters<PaperIdParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let notes = self
            .db
            .list_notes_for_paper(&params.paper_id)
            .await
            .map_err(|e| err(e))?;
        json_result(&notes)
    }

    #[tool(description = "List all collections in the library (hierarchical)")]
    async fn list_collections(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        let colls = self.db.list_collections().await.map_err(|e| err(e))?;
        json_result(&colls)
    }

    #[tool(description = "List all tags in the library")]
    async fn list_tags(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        let tags = self.db.list_tags().await.map_err(|e| err(e))?;
        json_result(&tags)
    }

    #[tool(description = "Get all papers in a specific collection")]
    async fn get_papers_in_collection(
        &self,
        Parameters(params): Parameters<CollectionIdParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let ids = self
            .db
            .list_paper_ids_in_collection(&params.collection_id)
            .await
            .map_err(|e| err(e))?;
        let mut papers = Vec::new();
        for id in ids {
            if let Some(p) = self.db.get_paper_by_id(&id).await.map_err(|e| err(e))? {
                papers.push(p);
            }
        }
        json_result(&papers)
    }

    #[tool(description = "Get all papers with a specific tag")]
    async fn get_papers_by_tag(
        &self,
        Parameters(params): Parameters<TagIdParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let ids = self
            .db
            .list_paper_ids_by_tag(&params.tag_id)
            .await
            .map_err(|e| err(e))?;
        let mut papers = Vec::new();
        for id in ids {
            if let Some(p) = self.db.get_paper_by_id(&id).await.map_err(|e| err(e))? {
                papers.push(p);
            }
        }
        json_result(&papers)
    }

    #[tool(description = "Extract plain text from a paper's PDF. Returns text per page. Uses cached text if available.")]
    async fn extract_pdf_text(
        &self,
        Parameters(params): Parameters<ExtractPdfTextParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let paper = self
            .db
            .get_paper_by_id(&params.paper_id)
            .await
            .map_err(|e| err(e))?
            .ok_or_else(|| err(format!("No paper found with ID {}", params.paper_id)))?;

        let pdf_path = paper
            .links
            .pdf_path
            .as_ref()
            .ok_or_else(|| err("Paper has no PDF file"))?;

        let page_indices: Vec<u32> = match params.pages {
            Some(pages) => {
                let mut p = pages;
                p.truncate(50);
                p
            }
            None => (0..10).collect(),
        };

        #[derive(Serialize)]
        struct PageText {
            page: u32,
            text: String,
        }

        // Try cached text first (extracted when the PDF was viewed in the app)
        // The app caches using a hash of the FULL absolute path
        let abs_pdf_path = self.db.resolve_pdf_path(pdf_path);
        let abs_path_str = abs_pdf_path.to_string_lossy().to_string();
        let data_dir = self.db.data_dir();
        let cache_key = {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            abs_path_str.hash(&mut hasher);
            format!("{:016x}", hasher.finish())
        };
        let text_cache = data_dir.join("cache").join(&cache_key).join("text.json");

        if text_cache.exists() {
            if let Ok(text_str) = std::fs::read_to_string(&text_cache) {
                if let Ok(cached) = serde_json::from_str::<Vec<serde_json::Value>>(&text_str) {
                    let pages: Vec<PageText> = cached
                        .iter()
                        .filter_map(|entry| {
                            let page_idx = entry.get("page_index")?.as_u64()? as u32;
                            if !page_indices.contains(&page_idx) {
                                return None;
                            }
                            // Concatenate all text segments on this page
                            let segments = entry.get("segments")?.as_array()?;
                            let text: String = segments
                                .iter()
                                .filter_map(|seg| seg.get("text").and_then(|t| t.as_str()))
                                .collect::<Vec<_>>()
                                .join("");
                            Some(PageText { page: page_idx, text })
                        })
                        .collect();

                    if !pages.is_empty() {
                        return json_result(&pages);
                    }
                }
            }
        }

        // Fall back to pdfium extraction if available
        if !self.pdf_available {
            return Err(err(
                "No cached text available and PDF engine is not loaded. Open the paper in the viewer first to extract text.",
            ));
        }

        let results = tokio::task::spawn_blocking(move || {
            let engine = rotero_pdf::PdfEngine::new(None)
                .map_err(|e| format!("Failed to init PDF engine: {e}"))?;
            rotero_pdf::text_extract::extract_raw_text(
                engine.pdfium(),
                &abs_path_str,
                &page_indices,
            )
            .map_err(|e| format!("{e}"))
        })
        .await
        .map_err(|e| err(e))?
        .map_err(|e| err(e))?;

        let pages: Vec<PageText> = results
            .into_iter()
            .map(|(page, text)| PageText { page, text })
            .collect();

        json_result(&pages)
    }

    #[tool(description = "Add a note to a paper")]
    async fn add_note(
        &self,
        Parameters(params): Parameters<AddNoteParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let id = self
            .db
            .insert_note(&params.paper_id, &params.title, &params.body)
            .await
            .map_err(|e| err(e))?;
        json_result(&serde_json::json!({ "note_id": id, "success": true }))
    }

    #[tool(description = "Update an existing note")]
    async fn update_note(
        &self,
        Parameters(params): Parameters<UpdateNoteParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.db
            .update_note(&params.note_id, &params.title, &params.body)
            .await
            .map_err(|e| err(e))?;
        json_result(&serde_json::json!({ "success": true }))
    }

    #[tool(description = "Add a tag to a paper (creates the tag if it doesn't exist)")]
    async fn add_tag_to_paper(
        &self,
        Parameters(params): Parameters<AddTagToPaperParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let tag_id = self
            .db
            .get_or_create_tag(&params.tag_name, params.color.as_deref())
            .await
            .map_err(|e| err(e))?;
        self.db
            .add_tag_to_paper(&params.paper_id, &tag_id)
            .await
            .map_err(|e| err(e))?;
        json_result(&serde_json::json!({ "tag_id": tag_id, "success": true }))
    }

    #[tool(description = "Mark a paper as read or unread")]
    async fn set_paper_read(
        &self,
        Parameters(params): Parameters<SetPaperReadParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.db
            .set_read(&params.paper_id, params.is_read)
            .await
            .map_err(|e| err(e))?;
        json_result(&serde_json::json!({ "success": true }))
    }

    #[tool(description = "Mark a paper as favorite or unfavorite")]
    async fn set_paper_favorite(
        &self,
        Parameters(params): Parameters<SetPaperFavoriteParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.db
            .set_favorite(&params.paper_id, params.is_favorite)
            .await
            .map_err(|e| err(e))?;
        json_result(&serde_json::json!({ "success": true }))
    }

    #[tool(description = "Get papers related to a specific paper via shared tags, authors, collections, or journal. Returns relationship type, strength, and connected paper details.")]
    async fn get_paper_relationships(
        &self,
        Parameters(params): Parameters<GetPaperRelationshipsParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let papers = self.db.list_all_papers().await.map_err(|e| err(e))?;
        let tags = self.db.list_tags().await.map_err(|e| err(e))?;
        let paper_tags = self.db.list_all_paper_tags().await.map_err(|e| err(e))?;
        let paper_colls = self.db.list_all_paper_collections().await.map_err(|e| err(e))?;

        let filter = rotero_graph::data::GraphFilter::default();
        let edges = rotero_graph::edges::compute_edges(&papers, &tags, &paper_tags, &paper_colls, &filter);

        let paper_map: std::collections::HashMap<&str, &rotero_models::Paper> = papers
            .iter()
            .filter_map(|p| Some((p.id.as_deref()?, p)))
            .collect();

        let relationships: Vec<PaperRelationship> = edges
            .iter()
            .filter_map(|e| {
                let (other_id, is_source) = if e.source == params.paper_id {
                    (e.target.as_str(), true)
                } else if e.target == params.paper_id {
                    (e.source.as_str(), false)
                } else {
                    return None;
                };
                let _ = is_source;
                let other = paper_map.get(other_id)?;
                Some(PaperRelationship {
                    related_paper_id: other_id.to_string(),
                    related_paper_title: other.title.clone(),
                    relationship_type: format!("{:?}", e.rel_type),
                    label: e.label.clone(),
                    weight: e.weight,
                })
            })
            .collect();

        if relationships.is_empty() {
            Ok(CallToolResult::success(vec![Content::text(format!(
                "No relationships found for paper {}",
                params.paper_id
            ))]))
        } else {
            json_result(&relationships)
        }
    }

    #[tool(description = "Get the full paper relationship graph showing how all papers in the library are connected via shared tags, authors, collections, and journals. Returns nodes (papers) and edges (relationships with types and weights).")]
    async fn get_library_graph(
        &self,
        Parameters(params): Parameters<GetLibraryGraphParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let papers = self.db.list_all_papers().await.map_err(|e| err(e))?;
        let tags = self.db.list_tags().await.map_err(|e| err(e))?;
        let paper_tags = self.db.list_all_paper_tags().await.map_err(|e| err(e))?;
        let paper_colls = self.db.list_all_paper_collections().await.map_err(|e| err(e))?;

        let max_edges = params.max_edges.unwrap_or(100).min(500) as usize;
        let filter = rotero_graph::data::GraphFilter::default();
        let mut edges = rotero_graph::edges::compute_edges(&papers, &tags, &paper_tags, &paper_colls, &filter);
        edges.truncate(max_edges);

        // Collect node IDs that appear in edges
        let mut node_ids: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for e in &edges {
            node_ids.insert(&e.source);
            node_ids.insert(&e.target);
        }

        let nodes: Vec<GraphNode> = papers
            .iter()
            .filter(|p| p.id.as_deref().is_some_and(|id| node_ids.contains(id)))
            .map(|p| GraphNode {
                id: p.id.clone().unwrap_or_default(),
                title: p.title.clone(),
                authors: p.authors.clone(),
                year: p.year,
            })
            .collect();

        let graph_edges: Vec<GraphEdge> = edges
            .iter()
            .map(|e| GraphEdge {
                source: e.source.clone(),
                target: e.target.clone(),
                relationship_type: format!("{:?}", e.rel_type),
                label: e.label.clone(),
                weight: e.weight,
            })
            .collect();

        json_result(&LibraryGraph {
            nodes,
            edges: graph_edges,
        })
    }
}

// -- ServerHandler implementation --------------------------------------------

#[tool_handler]
impl ServerHandler for RoteroMcp {
    fn get_info(&self) -> rmcp::model::ServerInfo {
        use rmcp::model::{Implementation, ServerCapabilities, ServerInfo};
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .enable_prompts()
                .build(),
        )
        .with_server_info(Implementation::new("rotero-mcp", env!("CARGO_PKG_VERSION")))
        .with_instructions(
            "Rotero paper library MCP server. Search papers, read annotations and notes, \
             extract PDF text, and manage your academic paper library.",
        )
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, rmcp::ErrorData> {
        Ok(ListResourcesResult {
            meta: None,
            next_cursor: None,
            resources: vec![
                rmcp::model::RawResource::new("rotero://library/stats", "Library statistics")
                    .no_annotation(),
            ],
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, rmcp::ErrorData> {
        let uri = request.uri.as_str();
        if uri == "rotero://library/stats" {
            let stats = LibraryStats {
                total_papers: self.db.count_papers().await.map_err(|e| err(e))?,
                total_collections: self.db.count_collections().await.map_err(|e| err(e))?,
                total_tags: self.db.count_tags().await.map_err(|e| err(e))?,
                unread_count: self.db.count_unread().await.map_err(|e| err(e))?,
                favorites_count: self.db.count_favorites().await.map_err(|e| err(e))?,
            };
            let json = serde_json::to_string_pretty(&stats).map_err(|e| err(e))?;
            Ok(ReadResourceResult::new(vec![ResourceContents::text(
                json, uri,
            )]))
        } else {
            Err(rmcp::ErrorData::invalid_params(
                format!("Unknown resource: {uri}"),
                None,
            ))
        }
    }

    async fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, rmcp::ErrorData> {
        Ok(ListPromptsResult {
            meta: None,
            next_cursor: None,
            prompts: vec![
                Prompt::new(
                    "summarize-paper",
                    Some("Summarize a paper from your library"),
                    Some(vec![
                        rmcp::model::PromptArgument::new("paper_id")
                            .with_description("Paper ID to summarize")
                            .with_required(true),
                    ]),
                ),
                Prompt::new(
                    "literature-review",
                    Some("Conduct a literature review on a topic using papers in your library"),
                    Some(vec![
                        rmcp::model::PromptArgument::new("topic")
                            .with_description("Topic to review")
                            .with_required(true),
                    ]),
                ),
            ],
        })
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResult, rmcp::ErrorData> {
        match request.name.as_str() {
            "summarize-paper" => {
                let paper_id = request
                    .arguments
                    .as_ref()
                    .and_then(|args| args.get("paper_id"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        rmcp::ErrorData::invalid_params("Missing paper_id argument", None)
                    })?;

                let paper = self
                    .db
                    .get_paper_by_id(paper_id)
                    .await
                    .map_err(|e| err(e))?
                    .ok_or_else(|| err(format!("No paper found with ID {paper_id}")))?;
                let anns = self
                    .db
                    .list_annotations_for_paper(paper_id)
                    .await
                    .map_err(|e| err(e))?;

                let mut prompt = format!(
                    "Please summarize the following paper:\n\n\
                     Title: {}\n\
                     Authors: {}\n\
                     Year: {}\n\
                     Journal: {}\n",
                    paper.title,
                    paper.authors.join(", "),
                    paper.year.map(|y| y.to_string()).unwrap_or_default(),
                    paper.publication.journal.as_deref().unwrap_or(""),
                );
                if let Some(abstract_text) = &paper.abstract_text {
                    prompt.push_str(&format!("\nAbstract:\n{abstract_text}\n"));
                }
                if !anns.is_empty() {
                    prompt.push_str("\nHighlights and annotations:\n");
                    for ann in &anns {
                        if let Some(content) = &ann.content {
                            prompt.push_str(&format!("- [p{}] {}\n", ann.page, content));
                        }
                    }
                }

                Ok(GetPromptResult::new(vec![PromptMessage::new_text(
                    PromptMessageRole::User,
                    prompt,
                )])
                .with_description("Summarize a paper"))
            }
            "literature-review" => {
                let topic = request
                    .arguments
                    .as_ref()
                    .and_then(|args| args.get("topic"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        rmcp::ErrorData::invalid_params("Missing topic argument", None)
                    })?;

                let papers = self.db.search_papers(topic).await.map_err(|e| err(e))?;

                let mut prompt = format!(
                    "Conduct a literature review on '{topic}' based on the following papers from my library:\n\n"
                );
                for paper in papers.iter().take(20) {
                    prompt.push_str(&format!(
                        "## {} ({})\n**Authors:** {}\n**Journal:** {}\n",
                        paper.title,
                        paper.year.map(|y| y.to_string()).unwrap_or_default(),
                        paper.authors.join(", "),
                        paper.publication.journal.as_deref().unwrap_or(""),
                    ));
                    if let Some(abstract_text) = &paper.abstract_text {
                        prompt.push_str(&format!("**Abstract:** {abstract_text}\n"));
                    }
                    prompt.push('\n');
                }
                if papers.is_empty() {
                    prompt.push_str("No papers found matching this topic in the library.\n");
                }

                Ok(GetPromptResult::new(vec![PromptMessage::new_text(
                    PromptMessageRole::User,
                    prompt,
                )])
                .with_description("Literature review"))
            }
            _ => Err(rmcp::ErrorData::invalid_params(
                format!("Unknown prompt: {}", request.name),
                None,
            )),
        }
    }
}

impl RoteroMcp {
    pub fn new(db: crate::db::Database, pdf_available: bool) -> Self {
        Self {
            db,
            pdf_available,
            tool_router: Self::tool_router(),
        }
    }
}
