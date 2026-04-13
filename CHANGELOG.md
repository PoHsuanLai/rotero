# Changelog

## Unreleased

### Fixed
- Context menu no longer truncated when opened near the bottom or right edge of the window

### Added
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
