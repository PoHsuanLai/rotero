use rmcp::{
    RoleServer, ServerHandler,
    handler::server::tool::ToolRouter,
    handler::server::wrapper::Parameters,
    model::{
        AnnotateAble, CallToolResult, Content, GetPromptRequestParams, GetPromptResult,
        Implementation, ListPromptsResult, ListResourcesResult, PaginatedRequestParams, Prompt,
        PromptMessage, PromptMessageRole, ReadResourceRequestParams, ReadResourceResult,
        ResourceContents, ServerCapabilities, ServerInfo,
    },
    schemars,
    service::RequestContext,
    tool, tool_handler, tool_router,
};
use serde::{Deserialize, Serialize};

use crate::db::Database;

pub struct RoteroMcp {
    db: Database,
    /// Whether pdfium is available (checked at startup).
    pdf_available: bool,
    tool_router: ToolRouter<Self>,
}

// -- Tool parameter structs --------------------------------------------------

#[derive(Deserialize, schemars::JsonSchema)]
pub struct SearchPapersParams {
    /// Search query string (searches title, authors, abstract, full text)
    pub query: String,
    /// Maximum number of results (default 20, max 50)
    pub limit: Option<u32>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct GetPaperParams {
    /// Paper ID
    pub paper_id: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ListPapersParams {
    /// Offset for pagination (default 0)
    pub offset: Option<u32>,
    /// Number of papers to return (default 20, max 100)
    pub limit: Option<u32>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct PaperIdParams {
    /// Paper ID
    pub paper_id: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct CollectionIdParams {
    /// Collection ID
    pub collection_id: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct TagIdParams {
    /// Tag ID
    pub tag_id: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ExtractPdfTextParams {
    /// Paper ID
    pub paper_id: String,
    /// Page numbers to extract (0-indexed). If omitted, extracts first 10 pages.
    pub pages: Option<Vec<u32>>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct AddNoteParams {
    /// Paper ID to add note to
    pub paper_id: String,
    /// Note title
    pub title: String,
    /// Note body text
    pub body: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct UpdateNoteParams {
    /// Note ID to update
    pub note_id: String,
    /// New title
    pub title: String,
    /// New body text
    pub body: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct AddTagToPaperParams {
    /// Paper ID
    pub paper_id: String,
    /// Tag name (will be created if it doesn't exist)
    pub tag_name: String,
    /// Optional tag color (hex, e.g. "#ff0000")
    pub color: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct SetPaperReadParams {
    /// Paper ID
    pub paper_id: String,
    /// Whether the paper is read
    pub is_read: bool,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct SetPaperFavoriteParams {
    /// Paper ID
    pub paper_id: String,
    /// Whether the paper is a favorite
    pub is_favorite: bool,
}

// -- Helpers -----------------------------------------------------------------

#[derive(Serialize)]
struct LibraryStats {
    total_papers: u32,
    total_collections: u32,
    total_tags: u32,
    unread_count: u32,
    favorites_count: u32,
}

fn err(msg: impl std::fmt::Display) -> rmcp::ErrorData {
    rmcp::ErrorData::internal_error(msg.to_string(), None)
}

fn json_result<T: Serialize>(value: &T) -> Result<CallToolResult, rmcp::ErrorData> {
    let text = serde_json::to_string_pretty(value).map_err(|e| err(e))?;
    Ok(CallToolResult::success(vec![Content::text(text)]))
}

// -- Tool implementations ----------------------------------------------------

#[tool_router]
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

    #[tool(description = "Extract plain text from a paper's PDF. Returns text per page.")]
    async fn extract_pdf_text(
        &self,
        Parameters(params): Parameters<ExtractPdfTextParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        if !self.pdf_available {
            return Err(err(
                "PDF engine not available. Set PDFIUM_DYNAMIC_LIB_PATH to enable PDF text extraction.",
            ));
        }

        let paper = self
            .db
            .get_paper_by_id(&params.paper_id)
            .await
            .map_err(|e| err(e))?
            .ok_or_else(|| err(format!("No paper found with ID {}", params.paper_id)))?;

        let pdf_path = paper
            .pdf_path
            .as_ref()
            .ok_or_else(|| err("Paper has no PDF file"))?;
        let abs_path = self.db.resolve_pdf_path(pdf_path);
        let abs_path_str = abs_path.to_string_lossy().to_string();

        let page_indices: Vec<u32> = match params.pages {
            Some(pages) => {
                let mut p = pages;
                p.truncate(50);
                p
            }
            None => (0..10).collect(),
        };

        // PdfEngine is not Send/Sync, so we create it on the blocking thread
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

        #[derive(Serialize)]
        struct PageText {
            page: u32,
            text: String,
        }
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
}

// -- ServerHandler implementation --------------------------------------------

#[tool_handler]
impl ServerHandler for RoteroMcp {
    fn get_info(&self) -> ServerInfo {
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
                    paper.journal.as_deref().unwrap_or(""),
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
                        paper.journal.as_deref().unwrap_or(""),
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
    pub fn new(db: Database, pdf_available: bool) -> Self {
        Self {
            db,
            pdf_available,
            tool_router: Self::tool_router(),
        }
    }
}
