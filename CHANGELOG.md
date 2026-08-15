# Changelog

## v0.2.0

The Node translation server is gone. Web imports now run entirely in-process in Rust, so there's no sidecar to install, start, or crash. This release also makes PDF citations navigable: links jump within a document, resolve to papers in your library, and feed a new citation graph.

### Added

**Citations and links**
- **Clickable PDF links** — internal links jump within the document; external links open the matching paper in your library, or fall back to the browser
- **Citation preview card** — click a citation to see the reference inline, with metadata fetched from OpenAlex/CrossRef/arXiv when the paper isn't in your library
- **Citations graph mode** — a 5th edge type showing directed citing → cited relationships between your papers
- `paper_citations` table (CRR-synced) populated by a one-time background scan of extracted PDF links
- DOI normalization: arXiv identifiers now match across both `arXiv:X` and `10.48550/arXiv.X` forms

**Web import (in-process translators)**
- Upstream Zotero translators run in-process via an embedded `boa` JS engine, backed by the vendored `zotero/utilities` corpus
- Rust-native hub translators: Embedded Metadata, DOI Content Negotiation, and a generic scraper
- The browser extension sends page HTML directly, so the connector skips a re-fetch — this makes gated publishers (Cloudflare-protected pages) work
- Browser-proxied authenticated fetch for publishers requiring a logged-in session
- Differential test harness across a 14-publisher corpus, validating Rust output against Zotero's own translator results

**Library**
- **Unified search bar** — one streaming, ranked bar over local papers and the web, with AND-based FTS matching
- Web-result overview with a compact "From the web" chip, and more robust import-download
- Tag and collection membership editing directly in the paper overview
- Nested collections in the browser extension, with an aggregate parent view
- Rebindable keyboard shortcuts settings page, built on a unified Command table with scoped dispatch
- Richer paper model: item types (including preprints), typed creators, and venue fields
- NBIB/RIS/BibTeX import routed by Zotero item type, with DOI validation

### Changed
- Settings modal redesigned: stable sizing, left tab rail, reorganized tabs
- CRR sync schema is now derived from typed structs instead of hand-maintained
- In-tree CRR module replaced by the external `recrr` crate
- iCloud sync is hidden in builds without the `cloudkit` feature, instead of appearing as a selectable option that silently did nothing
- App icons flattened onto a pure white background

### Fixed
- PDF viewer memory bounded via lazy windowed rendering; ACP agents are reaped on exit
- Text-input keybindings no longer swallow typing, via a focus-aware door-check
- Context menu visibility, right-click selection, and sidebar text selection
- Collection/tag deletion no longer cancelled by menu auto-close
- 10 incorrect Bootstrap Icon code points
- arXiv and other identifiers resolve to their real URLs rather than `doi.org`
- Full-text cache is re-saved after background extraction on first open
- Databases stamped v11 without the `item_type` column are healed on open

## v0.1.6

### Fixed
- AI agent Node.js subprocess now exits cleanly when Rotero closes (previously could leak as orphaned processes that accumulated across sessions)

## v0.1.5

### Fixed
- Context menu no longer truncated when opened near the bottom or right edge of the window
- Semantic Scholar citation count fetch no longer 404s for papers with `10.48550/arXiv.*` DOIs
- Zotero translation server now starts reliably (fixed missing `current_dir` causing config load failure)
- Translation server startup captures stderr on crash/timeout for easier debugging

### Added
- **Improved OA PDF search:** queries Zotero translation server, OpenAlex, Semantic Scholar, and Unpaywall in sequence
- **Agent fallback for PDF search:** when automated sources fail, "Find PDF" button becomes "Ask Agent" to delegate web search to the AI agent
- **`download_pdf` MCP tool:** agents can now download a PDF from a URL and attach it to an existing paper
- OA search status persists per-paper — switch between papers while searches run in background
- **Unified `PaperId` enum** for parsing paper identifiers (DOI, arXiv, PMID, ISBN) — replaces scattered string-prefix checks with a single typed parser
- Zotero translator now preserves ISBN and extracts PMID from the `extra` field when DOI is absent
- LaTeX math rendering in chat messages, paper abstracts, and note previews via `pulldown-latex` (pure Rust MathML, no JS runtime)
- **Multi-select in library view:** Cmd+Click to toggle, Shift+Click for range select, Cmd+A to select all
- **Keyboard shortcuts:** Arrow keys to navigate papers, Enter to open PDF, Delete/Backspace to delete with confirmation, Cmd+Shift+F to toggle favorite, Cmd+Shift+U to toggle read/unread, Escape to clear selection
- **Bulk operations:** context menu actions (favorite, read/unread, copy DOIs, remove from collection, delete) apply to all selected papers
- **Multi-select detail panel:** shows selected paper cards with bulk action buttons and per-paper deselect
- **Delete confirmation dialog:** all deletes now require confirmation
- **Multi-drag:** drag multiple selected papers onto collections or tags

### Changed
- Compact sidebar tags: smaller font, tighter padding, removed icon for better stacking

## v0.1.4

### Fixed
- Blurry PDF rendering on HiDPI/Retina displays — DPR is now read from the native window scale factor synchronously at startup instead of racing with an async JS eval

### Added
- 12 new MCP write tools for full library management via AI agents:
  - **Papers:** `add_paper`, `update_paper`, `delete_paper`, `remove_tag_from_paper`
  - **Collections:** `create_collection`, `add_paper_to_collection`, `remove_paper_from_collection`, `delete_collection`, `rename_collection`
  - **Tags:** `rename_tag`, `delete_tag`
  - **Notes:** `delete_note`
- CRR sync tracking on all new MCP write operations
- Word add-in for citation management in Microsoft Word:
  - Insert inline citations from your Rotero library
  - Generate bibliography from all cited papers
  - Refresh all citations/bibliography to a new style
  - Taskpane served from the connector; icons hosted on GitHub Pages
- Citation API on the browser connector (port 21984):
  - `GET /api/cite/styles` — list available CSL citation styles
  - `GET /api/cite/search` — search papers in library
  - `POST /api/cite/format` — generate inline citations
  - `POST /api/cite/bibliography` — generate formatted bibliography entries
- `format_inline_citations()` and `format_bibliography_entries()` in rotero-bib
- `get_papers_by_ids()` bulk fetch in rotero-db
- Improved app restart after update (uses bundle identifier via `open -b`)
- MCP tag/collection tools now accept arrays for batch operations in a single call
- UI auto-refreshes after MCP write operations (papers, tags, collections, notes)

## v0.1.3

### Added
- NBIB (PubMed/MEDLINE) import support
- Cargo doc comments across all 9 workspace crates
- `FromRow` trait, `collect_rows` helper, and shared row/value helpers
- `SyncBackend` trait for future sync backends
- In-app startup update check

### Changed
- Replace `std::sync::mpsc` with `tokio::sync::oneshot` for render replies
- Refactor: extract shared helpers, decompose `panel.rs`, fix non-idiomatic patterns
- Fix all clippy warnings

### Fixed
- Restart after update: detect `.app` bundle vs dev build

## v0.1.2

### Added
- In-app auto-update via GitHub Releases
- Sort button in library panel

## v0.1.1

### Fixed
- MCP `extract_pdf_text`: save complete fulltext and add pagination
- Re-extract text for pages with missing `text_data` on cache hit
- Extract `save_fulltext_to_db` helper, fix cache-hit path

## v0.1.0

Initial release.
