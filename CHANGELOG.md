# Changelog

## v0.2.3

If you run Rotero on more than one machine, this is the important one. Several bugs meant a device could stop syncing entirely — silently, while reporting success — and that tags, notes, and merged papers could be lost between devices. All of them are fixed, and libraries damaged by earlier versions repair themselves on first launch.

The common thread is that the shipped app took a different code path than the tests did. Everything below was found by auditing that gap.

There is also a user guide now.

### Added
- **A documentation site with a written user guide**, covering the library, the PDF reader, the browser extension, the Word add-in, sync, and the AI assistant — with screenshots captured from a real build, and a coverage check that reports which parts of the app the guide does not yet describe

### Fixed

**Sync and data loss**
- **A device could stop syncing forever, and say it was fine.** All devices shared one `sync_state.json`, but the export cursor in it is private to each device. Whichever device synced first parked the cursor at its own position; every slower device then found "nothing newer" and sent nothing — permanently, since the cursor only moves forward. Each device now keeps its own state file, and an affected one re-sends its full history on first launch.
- **Tags removed and re-added on a broken build never came back.** The repair pass added in v0.2.2 skipped exactly those, then recorded itself as complete so it never tried again. Locally the tag looked fine; other devices were told it had been deleted. The repair now runs once more on every library and handles the case correctly.
- **Merging duplicates lost the surviving paper's tags and collections everywhere else.** The memberships moved on the machine that did the merge and were never sent; other devices kept memberships pointing at the deleted paper, and further syncing could not fix it.
- **Deleting a paper left its annotations, notes, and memberships behind.** The database declared these should be removed automatically, but that behaviour was never switched on, so every deleted paper leaked rows on every device.
- **Notes and tags the AI assistant created never left the machine.** Four of its write paths saved locally without recording the change for sync.

**Crashes**
- The window could go blank with no error when a PDF annotation had malformed geometry, or when a note preview, an API key, an imported `.ris`/`.nbib` file, or a paper title contained a non-English character near a truncation point.

**PDF viewing**
- A missing PDF engine reported "channel closed" on every action, and left the reader spinning forever with no way to retry. It now says what actually happened, the rest of the app keeps working, and the startup banner reports it.

**The library list**
- Favouriting, deleting, or editing a paper **while a search was active** appeared to work and then reverted, because results were drawn from a stale snapshot. The right-click menu could show the opposite state at the same moment.
- arXiv papers found through a web search never showed as imported, so clicking Import again added a second copy. "Import All" re-imported everything if clicked twice.
- The graph only refreshed when you changed the edge type, ignoring papers added or removed in the meantime.
- A row could stay stuck on "Importing…" for the rest of the session.

**Settings**
- **Settings that could not be written silently reverted on the next launch**, including the AI API key — the field clears itself to confirm it saved, so the key looked stored while never reaching disk. Failures are now reported.

**Security and privacy**
- **Any website you visited could read and write your library.** The browser connector accepted requests from any origin with no authentication; being bound to localhost is not a defence, since the browser is itself a local process. It now requires a token that the extension and Word add-in obtain automatically — you should not notice the change.
- A synced PDF path could read or write files outside the library folder.
- `~/rotero-debug.log` recorded the full URL of every page scraped — a plaintext browsing history, world-readable, including session tokens carried in publisher URLs. Only the site name is logged now, the file is private to your account, and `RUST_LOG` is honoured.
- `config.json`, which stores AI API keys in plaintext, was readable by any other account on the machine.

**Node.js (AI assistant)**
- A version-manager shim on `PATH` could hang the assistant permanently with no error.
- Installing bundled Node.js could report a spurious timeout on a perfectly good connection, and a crash mid-install could leave no working Node.js at all.

### Changed
- The health check now detects a library whose rows exist but are not being tracked for sync — the exact state earlier versions left behind, previously reported as healthy
- The bundle smoke test verifies a tag survives a restart, and that the connector refuses unauthenticated requests, against the real packaged app
- Lint failures now fail CI; previously they were printed and ignored
- 267 tests, up from 232. Every sync fix is verified across two simulated devices, because a single-device check passes whether or not the data would actually sync

## v0.2.2

Ships the Windows and Linux installers that v0.2.1 built but failed to publish. Everything else in v0.2.1 is unchanged — if you already run it, there is nothing new here beyond the installers.

### Fixed
- **Windows `.msi` and Linux `.deb` are now published.** They were built correctly in v0.2.1, but the release job looked for them in the wrong directory and shipped without them rather than failing. The portable `.zip`/`.tar.gz` were unaffected.

### Changed
- Release builds fetch a prebuilt Dioxus CLI instead of compiling it, cutting roughly 8 minutes from each of the three build jobs

## v0.2.1

Rotero now builds, ships, and updates itself on Windows and Linux, not just macOS. Getting there meant replacing the places that shelled out to platform binaries with portable Rust — which also fixed four real bugs, one of them on macOS.

### Added
- **Windows and Linux builds** — releases now include a Windows `.zip` and `.msi`, and a Linux `.tar.gz` and `.deb`, alongside the macOS `.dmg`
- **In-app updates on every platform** — Windows and Linux swap the running executable via `self-replace`; macOS keeps the `.app` bundle swap
- CI now compiles and tests on Windows and macOS, not only Linux — the reason the Windows break went unnoticed for so long
- One-click download on the website: the macOS button resolves the real installer URL instead of dropping you on the releases page

### Fixed
- **Quitting could hang instead of exiting (macOS, Linux).** The agent reaper's signal handler locked a mutex and allocated inside a real signal context; a signal arriving while a child process was being registered would deadlock the handler.
- **The AI agent leaked processes on Windows.** Children are now spawned into a job object, so the agent and anything it spawned are cleaned up together — previously only the immediate child was killed.
- **Bundled Node.js install was broken on Windows.** The archive unpacked into a nested directory while the launcher looked for the binary at the top level, so setup failed for anyone without Node.js already installed.
- **`npm` could not be run on Windows**, because it ships as a batch file, which needs `cmd.exe`.
- **Update checks failed on Windows and Linux** with a confusing message about a missing macOS file. Each platform now looks for its own build, and platforms without one say so and offer the releases page.
- Update failures no longer report "Failed to check for updates" when it was the *install* that failed, and no longer show raw network/filesystem errors — each failure now explains what to do next.
- A pre-release tag (`v0.3.0-rc1`) is no longer offered as an upgrade over the matching final release.
- The auto-fix CI workflow interpolated a pull-request branch name directly into a shell command, so a specially-named branch could run arbitrary commands in CI.

### Changed
- Node.js is downloaded and unpacked in-process; `curl` and `tar` are no longer required on `PATH`
- An unsupported OS or CPU now reports that clearly instead of silently downloading the Windows x64 build
- Dependencies updated across the workspace, including nine major versions (`scraper`, `lopdf`, `biblatex`, `hayagriva`, `rfd`, `base64`, `tower-http`, `ego-tree`, `pulldown-latex`)
- First unit tests in the app crate (18), covering the process registry, archive extraction, version comparison, and platform asset selection

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
