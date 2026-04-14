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

/// Convert any displayable error into an MCP internal error response.
pub(super) fn err(msg: impl std::fmt::Display) -> rmcp::ErrorData {
    rmcp::ErrorData::internal_error(msg.to_string(), None)
}

/// Serialize a value as pretty-printed JSON and wrap it in a successful tool result.
pub(super) fn json_result<T: Serialize>(value: &T) -> Result<CallToolResult, rmcp::ErrorData> {
    let text = serde_json::to_string_pretty(value).map_err(err)?;
    Ok(CallToolResult::success(vec![Content::text(text)]))
}

#[rmcp::tool_router]
impl RoteroMcp {
    #[tool(description = "Search papers in the library by title, authors, abstract, or full text")]
    async fn search_papers(
        &self,
        Parameters(params): Parameters<SearchPapersParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let mut papers = self.db.search_papers(&params.query).await.map_err(err)?;
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
            .map_err(err)?;
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
        let total = self.db.count_papers().await.map_err(err)?;
        let papers = self.db.list_papers(offset, limit).await.map_err(err)?;

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
            .map_err(err)?;
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
            .map_err(err)?;
        json_result(&notes)
    }

    #[tool(description = "List all collections in the library (hierarchical)")]
    async fn list_collections(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        let colls = self.db.list_collections().await.map_err(err)?;
        json_result(&colls)
    }

    #[tool(description = "List all tags in the library")]
    async fn list_tags(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        let tags = self.db.list_tags().await.map_err(err)?;
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
            .map_err(err)?;
        let mut papers = Vec::new();
        for id in ids {
            if let Some(p) = self.db.get_paper_by_id(&id).await.map_err(err)? {
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
            .map_err(err)?;
        let mut papers = Vec::new();
        for id in ids {
            if let Some(p) = self.db.get_paper_by_id(&id).await.map_err(err)? {
                papers.push(p);
            }
        }
        json_result(&papers)
    }

    #[tool(
        description = "Extract plain text from a paper's PDF with pagination. Returns a page range (default: first 10 pages) and total page count so you can request more. Use page_start/page_end to navigate."
    )]
    async fn extract_pdf_text(
        &self,
        Parameters(params): Parameters<ExtractPdfTextParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        // 1. Try cached text.json (has per-page structure for pagination)
        let paper = self
            .db
            .get_paper_by_id(&params.paper_id)
            .await
            .map_err(err)?
            .ok_or_else(|| err(format!("No paper found with ID {}", params.paper_id)))?;

        if let Some(pdf_path) = paper.links.pdf_path.as_ref() {
            let abs_path = self
                .db
                .resolve_pdf_path(pdf_path)
                .to_string_lossy()
                .to_string();
            let cache_key = {
                use std::hash::{Hash, Hasher};
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                abs_path.hash(&mut hasher);
                format!("{:016x}", hasher.finish())
            };
            let text_cache = self
                .db
                .data_dir()
                .join("cache")
                .join(&cache_key)
                .join("text.json");

            if text_cache.exists()
                && let Ok(text_str) = std::fs::read_to_string(&text_cache)
                && let Ok(mut cached) = serde_json::from_str::<Vec<serde_json::Value>>(&text_str)
            {
                // Sort by page_index
                cached.sort_by_key(|entry| {
                    entry
                        .get("page_index")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0)
                });
                let total_pages = cached.len() as u32;

                if total_pages > 0 {
                    let page_start = params.page_start.unwrap_or(1).max(1);
                    let page_end = params.page_end.unwrap_or(page_start + 9).min(total_pages);
                    let page_start = page_start.min(total_pages);

                    let text: String = cached
                        .iter()
                        .skip((page_start - 1) as usize)
                        .take((page_end - page_start + 1) as usize)
                        .filter_map(|entry| {
                            let segments = entry.get("segments")?.as_array()?;
                            let page_text: String = segments
                                .iter()
                                .filter_map(|seg| seg.get("text").and_then(|t| t.as_str()))
                                .collect::<Vec<_>>()
                                .join("");
                            Some(page_text)
                        })
                        .collect::<Vec<_>>()
                        .join("\n\n");

                    return json_result(&ExtractPdfTextResult {
                        text,
                        page_start,
                        page_end,
                        total_pages,
                    });
                }
            }
        }

        // 2. Fall back to DB fulltext (flat string, no pagination possible)
        let fulltext = self
            .db
            .get_paper_fulltext(&params.paper_id)
            .await
            .map_err(err)?;
        if let Some(text) = fulltext
            && !text.is_empty()
        {
            return json_result(&ExtractPdfTextResult {
                text,
                page_start: 1,
                page_end: 0,
                total_pages: 0,
            });
        }

        Err(err(
            "No extracted text available. Open the paper in the PDF viewer first.",
        ))
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
            .map_err(err)?;
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
            .map_err(err)?;
        json_result(&serde_json::json!({ "success": true }))
    }

    #[tool(
        description = "Add one or more tags to one or more papers (creates tags if they don't exist)"
    )]
    async fn add_tag_to_paper(
        &self,
        Parameters(params): Parameters<AddTagToPaperParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let mut tag_ids = Vec::new();
        for tag_name in &params.tag_names {
            let tag_id = self
                .db
                .get_or_create_tag(tag_name, params.color.as_deref())
                .await
                .map_err(err)?;
            tag_ids.push(tag_id);
        }
        for paper_id in &params.paper_ids {
            for tag_id in &tag_ids {
                self.db
                    .add_tag_to_paper(paper_id, tag_id)
                    .await
                    .map_err(err)?;
            }
        }
        json_result(&serde_json::json!({ "tag_ids": tag_ids, "success": true }))
    }

    #[tool(description = "Mark a paper as read or unread")]
    async fn set_paper_read(
        &self,
        Parameters(params): Parameters<SetPaperReadParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.db
            .set_read(&params.paper_id, params.is_read)
            .await
            .map_err(err)?;
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
            .map_err(err)?;
        json_result(&serde_json::json!({ "success": true }))
    }

    #[tool(
        description = "Add a new paper to the library with metadata. Returns the new paper ID. At minimum provide a title; other fields are optional."
    )]
    async fn add_paper(
        &self,
        Parameters(params): Parameters<AddPaperParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let paper = rotero_models::Paper {
            id: None,
            title: params.title,
            authors: params.authors.unwrap_or_default(),
            year: params.year,
            doi: params.doi,
            abstract_text: params.abstract_text,
            publication: rotero_models::Publication {
                journal: params.journal,
                volume: params.volume,
                issue: params.issue,
                pages: params.pages,
                publisher: params.publisher,
            },
            links: rotero_models::PaperLinks {
                url: params.url,
                pdf_url: params.pdf_url,
                pdf_path: None,
            },
            status: rotero_models::LibraryStatus::default(),
            citation: rotero_models::CitationInfo::default(),
        };
        let id = self.db.insert_paper(&paper).await.map_err(err)?;
        json_result(&serde_json::json!({ "paper_id": id, "success": true }))
    }

    #[tool(
        description = "Update a paper's metadata. Fetches the current paper, applies the provided fields (non-null only), and saves. Pass only the fields you want to change."
    )]
    async fn update_paper(
        &self,
        Parameters(params): Parameters<UpdatePaperParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let mut paper = self
            .db
            .get_paper_by_id(&params.paper_id)
            .await
            .map_err(err)?
            .ok_or_else(|| err(format!("No paper found with ID {}", params.paper_id)))?;

        if let Some(title) = params.title {
            paper.title = title;
        }
        if let Some(authors) = params.authors {
            paper.authors = authors;
        }
        if let Some(year) = params.year {
            paper.year = Some(year);
        }
        if let Some(doi) = params.doi {
            paper.doi = Some(doi);
        }
        if let Some(abstract_text) = params.abstract_text {
            paper.abstract_text = Some(abstract_text);
        }
        if let Some(journal) = params.journal {
            paper.publication.journal = Some(journal);
        }
        if let Some(volume) = params.volume {
            paper.publication.volume = Some(volume);
        }
        if let Some(issue) = params.issue {
            paper.publication.issue = Some(issue);
        }
        if let Some(pages) = params.pages {
            paper.publication.pages = Some(pages);
        }
        if let Some(publisher) = params.publisher {
            paper.publication.publisher = Some(publisher);
        }
        if let Some(url) = params.url {
            paper.links.url = Some(url);
        }

        self.db
            .update_paper_metadata(&params.paper_id, &paper)
            .await
            .map_err(err)?;
        json_result(&serde_json::json!({ "success": true }))
    }

    #[tool(
        description = "Delete a paper from the library. This permanently removes the paper and all its annotations, notes, and collection/tag associations."
    )]
    async fn delete_paper(
        &self,
        Parameters(params): Parameters<DeletePaperParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.db.delete_paper(&params.paper_id).await.map_err(err)?;
        json_result(&serde_json::json!({ "success": true }))
    }

    #[tool(
        description = "Remove one or more tags from one or more papers (does not delete the tags themselves)"
    )]
    async fn remove_tag_from_paper(
        &self,
        Parameters(params): Parameters<RemoveTagFromPaperParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        for paper_id in &params.paper_ids {
            for tag_id in &params.tag_ids {
                self.db
                    .remove_tag_from_paper(paper_id, tag_id)
                    .await
                    .map_err(err)?;
            }
        }
        json_result(&serde_json::json!({ "success": true }))
    }

    #[tool(
        description = "Create a new collection (folder) for organizing papers. Optionally nest it under a parent collection."
    )]
    async fn create_collection(
        &self,
        Parameters(params): Parameters<CreateCollectionParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let id = self
            .db
            .insert_collection(&params.name, params.parent_id.as_deref())
            .await
            .map_err(err)?;
        json_result(&serde_json::json!({ "collection_id": id, "success": true }))
    }

    #[tool(description = "Add one or more papers to one or more collections")]
    async fn add_paper_to_collection(
        &self,
        Parameters(params): Parameters<AddPaperToCollectionParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        for paper_id in &params.paper_ids {
            for collection_id in &params.collection_ids {
                self.db
                    .add_paper_to_collection(paper_id, collection_id)
                    .await
                    .map_err(err)?;
            }
        }
        json_result(&serde_json::json!({ "success": true }))
    }

    #[tool(description = "Remove one or more papers from one or more collections")]
    async fn remove_paper_from_collection(
        &self,
        Parameters(params): Parameters<RemovePaperFromCollectionParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        for paper_id in &params.paper_ids {
            for collection_id in &params.collection_ids {
                self.db
                    .remove_paper_from_collection(paper_id, collection_id)
                    .await
                    .map_err(err)?;
            }
        }
        json_result(&serde_json::json!({ "success": true }))
    }

    #[tool(description = "Delete a collection (removes the collection but not the papers in it)")]
    async fn delete_collection(
        &self,
        Parameters(params): Parameters<DeleteCollectionParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.db
            .delete_collection(&params.collection_id)
            .await
            .map_err(err)?;
        json_result(&serde_json::json!({ "success": true }))
    }

    #[tool(description = "Rename a collection")]
    async fn rename_collection(
        &self,
        Parameters(params): Parameters<RenameCollectionParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.db
            .rename_collection(&params.collection_id, &params.name)
            .await
            .map_err(err)?;
        json_result(&serde_json::json!({ "success": true }))
    }

    #[tool(description = "Rename a tag")]
    async fn rename_tag(
        &self,
        Parameters(params): Parameters<RenameTagParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.db
            .rename_tag(&params.tag_id, &params.name)
            .await
            .map_err(err)?;
        json_result(&serde_json::json!({ "success": true }))
    }

    #[tool(description = "Delete a tag from the library (removes it from all papers)")]
    async fn delete_tag(
        &self,
        Parameters(params): Parameters<DeleteTagParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.db.delete_tag(&params.tag_id).await.map_err(err)?;
        json_result(&serde_json::json!({ "success": true }))
    }

    #[tool(description = "Delete a note by its ID")]
    async fn delete_note(
        &self,
        Parameters(params): Parameters<DeleteNoteParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.db.delete_note(&params.note_id).await.map_err(err)?;
        json_result(&serde_json::json!({ "success": true }))
    }

    #[tool(
        description = "Download a PDF from a URL and attach it to an existing paper in the library. Use this when you find an open access PDF link for a paper that doesn't have a PDF yet."
    )]
    async fn download_pdf(
        &self,
        Parameters(params): Parameters<DownloadPdfParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        // Verify paper exists
        let paper = self
            .db
            .get_paper_by_id(&params.paper_id)
            .await
            .map_err(err)?
            .ok_or_else(|| err(format!("No paper found with ID {}", params.paper_id)))?;

        // Download the PDF
        let client = reqwest::Client::builder()
            .user_agent("Mozilla/5.0 (compatible; Rotero/0.1)")
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()
            .map_err(|e| err(format!("HTTP client error: {e}")))?;

        let resp = client
            .get(&params.pdf_url)
            .send()
            .await
            .map_err(|e| err(format!("Download failed: {e}")))?;

        if !resp.status().is_success() {
            return Err(err(format!("HTTP {}", resp.status())));
        }

        let bytes = resp
            .bytes()
            .await
            .map_err(|e| err(format!("Failed to read response: {e}")))?;

        if !bytes.starts_with(b"%PDF") {
            return Err(err("URL did not return a valid PDF file"));
        }

        // Save to library
        let first_author = paper.authors.first().map(|s| s.as_str());
        let papers_dir = self.db.papers_dir();
        std::fs::create_dir_all(&papers_dir).map_err(|e| err(format!("Failed to create papers dir: {e}")))?;

        let safe_title: String = paper.title.chars()
            .map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { '_' })
            .collect();
        let safe_title = safe_title.trim();
        let safe_title = if safe_title.len() > 80 { &safe_title[..80] } else { safe_title };

        let filename = if let Some(author) = first_author {
            let safe_author: String = author.chars()
                .map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { '_' })
                .collect();
            if let Some(year) = paper.year {
                format!("{safe_author} - {year} - {safe_title}.pdf")
            } else {
                format!("{safe_author} - {safe_title}.pdf")
            }
        } else {
            format!("{safe_title}.pdf")
        };

        let dest = papers_dir.join(&filename);
        std::fs::write(&dest, &bytes).map_err(|e| err(format!("Failed to save PDF: {e}")))?;

        // Update the paper's pdf_path in the database
        self.db
            .update_pdf_path(&params.paper_id, &filename)
            .await
            .map_err(err)?;

        json_result(&serde_json::json!({
            "success": true,
            "pdf_path": filename,
            "size_bytes": bytes.len(),
        }))
    }

    #[tool(
        description = "Get papers related to a specific paper via shared tags, authors, collections, or journal. Returns relationship type, strength, and connected paper details."
    )]
    async fn get_paper_relationships(
        &self,
        Parameters(params): Parameters<GetPaperRelationshipsParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let papers = self.db.list_all_papers().await.map_err(err)?;
        let tags = self.db.list_tags().await.map_err(err)?;
        let paper_tags = self.db.list_all_paper_tags().await.map_err(err)?;
        let paper_colls = self.db.list_all_paper_collections().await.map_err(err)?;

        let filter = rotero_graph::data::GraphFilter::default();
        let edges =
            rotero_graph::edges::compute_edges(&papers, &tags, &paper_tags, &paper_colls, &filter);

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

    #[tool(
        description = "Get the full paper relationship graph showing how all papers in the library are connected via shared tags, authors, collections, and journals. Returns nodes (papers) and edges (relationships with types and weights)."
    )]
    async fn get_library_graph(
        &self,
        Parameters(params): Parameters<GetLibraryGraphParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let papers = self.db.list_all_papers().await.map_err(err)?;
        let tags = self.db.list_tags().await.map_err(err)?;
        let paper_tags = self.db.list_all_paper_tags().await.map_err(err)?;
        let paper_colls = self.db.list_all_paper_collections().await.map_err(err)?;

        let max_edges = params.max_edges.unwrap_or(100).min(500) as usize;
        let filter = rotero_graph::data::GraphFilter::default();
        let mut edges =
            rotero_graph::edges::compute_edges(&papers, &tags, &paper_tags, &paper_colls, &filter);
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
            "Rotero paper library MCP server. Search, add, update, and delete papers. \
             Manage collections and tags. Read annotations and notes, extract PDF text, \
             and organize your academic paper library.",
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
                total_papers: self.db.count_papers().await.map_err(err)?,
                total_collections: self.db.count_collections().await.map_err(err)?,
                total_tags: self.db.count_tags().await.map_err(err)?,
                unread_count: self.db.count_unread().await.map_err(err)?,
                favorites_count: self.db.count_favorites().await.map_err(err)?,
            };
            let json = serde_json::to_string_pretty(&stats).map_err(err)?;
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
                    .map_err(err)?
                    .ok_or_else(|| err(format!("No paper found with ID {paper_id}")))?;
                let anns = self
                    .db
                    .list_annotations_for_paper(paper_id)
                    .await
                    .map_err(err)?;

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

                let papers = self.db.search_papers(topic).await.map_err(err)?;

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
        }
    }
}
