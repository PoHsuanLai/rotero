# Rotero

<p align="center">
  <img src="assets/icon.png" alt="Rotero" width="128" />
</p>

<p align="center">
  <a href="https://pohsuanlai.github.io/rotero"><img src="https://img.shields.io/badge/website-rotero-teal" alt="Website"></a>
  <a href="https://github.com/PoHsuanLai/rotero/actions/workflows/ci.yml"><img src="https://github.com/PoHsuanLai/rotero/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://github.com/PoHsuanLai/rotero/releases/latest"><img src="https://img.shields.io/github/v/release/PoHsuanLai/rotero" alt="Release"></a>
  <a href="https://github.com/PoHsuanLai/rotero/blob/master/LICENSE"><img src="https://img.shields.io/github/license/PoHsuanLai/rotero" alt="License"></a>
  <img src="https://img.shields.io/badge/rust-1.93+-orange?logo=rust" alt="Rust">
  <img src="https://img.shields.io/badge/dioxus-0.7-blue" alt="Dioxus">
  <img src="https://img.shields.io/badge/turso-0.6-teal" alt="Turso">
</p>

A fast, private, local-first reference manager built with Rust. Read, annotate, cite, and explore your papers — without the bloat.

<p align="center">
  <img src="assets/screenshot-reader.png" alt="Rotero PDF Reader" width="800" />
</p>

## Why Rotero
- **Zotero web translators compatible** — One-click import from Google Scholar, arXiv, PubMed, and 40+ academic sites
- **Citation graph** — Interactive visualization of how your papers connect
- **AI research assistant** — Chat with your papers via ACP — use your Claude subscription, no API costs
- **CRR sync** — Custom conflict-free replicated relations for multi-device sync
- **Local-first** — SQLite database, no accounts, no telemetry, no cloud dependency

## Performance

Memory with 5 PDF tabs open (avg of 5 runs, macOS):

| | Rotero | Zotero 7 |
|---|---|---|
| Memory | ~220 MB | ~1.4 GB |

## Status

Under active development. Known limitations:

- PDF virtual text layer (selection/copy) needs refinement
- Mobile app (iOS/Android) planned, not yet available

## Install

Download the latest release from the [Releases page](https://github.com/PoHsuanLai/rotero/releases/latest).

> **macOS note:** On first launch, macOS may show "Rotero can't be opened because Apple cannot check it for malicious software." This is because the app is not notarized with an Apple Developer account. To open it: right-click the app → Open → Open. You only need to do this once.

### Build from source

Requires [Rust](https://rustup.rs/) and [just](https://github.com/casey/just).

```sh
git clone https://github.com/PoHsuanLai/rotero.git
cd rotero
just run    # downloads PDFium, builds, runs
```

Other commands: `just check`, `just lint`, `just build-release`, `just run-release`, `just clean`

## Browser Extension

Download `Rotero-Extension.zip` from the [Releases page](https://github.com/PoHsuanLai/rotero/releases/latest), unzip it, then:

1. Go to `chrome://extensions/` → enable **Developer mode** (top right)
2. Click **Load unpacked** → select the unzipped folder
3. Keep Rotero running (the extension connects to `localhost:21984`)

> **Why Developer mode?** The extension is not on the Chrome Web Store, which requires a paid developer account. Loading unpacked is the standard way to install extensions distributed outside the store.

## Architecture

Cargo workspace with 9 crates:

| Crate | Purpose | Key deps |
|---|---|---|
| `rotero-models` | Shared data types | serde |
| `rotero-db` | SQLite CRUD | turso |
| `rotero-pdf` | PDF rendering + annotation writing | pdfium-render, lopdf |
| `rotero-bib` | BibTeX/RIS/CSL + citation generation | biblatex, hayagriva |
| `rotero-connector` | Browser extension HTTP server | axum |
| `rotero-translate` | Zotero translation server (Node.js sidecar) | reqwest |
| `rotero-graph` | Citation graph visualization | fdg |
| `rotero-mcp` | MCP server for AI integration | rmcp |
| `rotero` (app) | Desktop UI, state management | dioxus, reqwest |

## Data Storage

Local SQLite database. PDFs copied to a managed directory.

| Platform | Location |
|---|---|
| macOS | `~/Library/Application Support/com.rotero.Rotero/` |
| Linux | `~/.local/share/rotero/` |

## License

AGPL-3.0-or-later
