---
layout: docs
title: MCP server
description: Expose your Rotero library to Claude Desktop, Claude Code, and other MCP clients — 32 tools, one resource, two prompts.
---

<script>
  import { base } from '$app/paths';
  import Callout from '$lib/components/docs/Callout.svelte';
</script>

Rotero speaks the Model Context Protocol, so any MCP client can search your
papers, read your annotations, and file things into collections.

There are two ways to run it.

## Embedded in the app

Rotero serves MCP at `http://127.0.0.1:21985/mcp` while the app is running. This
is what the [in-app assistant]({base}/docs/ai-assistant) connects to, with no
setup on your part.

<Callout type="note">

`21985`, not `21984` — the [connector]({base}/docs/connector-api) has the lower
port. PDF text extraction is disabled in embedded mode; every other tool works.

</Callout>

## Standalone server

For Claude Desktop, Claude Code, or anything else that launches an MCP server
over stdio, build the `rotero-mcp` binary from a
[source checkout]({base}/docs/install#building-from-source):

```sh
cargo build --release -p rotero-mcp
```

The binary lands at `target/release/rotero-mcp`.

Configure your client with an **absolute** path — MCP servers do not inherit a
useful working directory. This is the repository's own `.mcp.json`:

```json
{
  "mcpServers": {
    "rotero": {
      "type": "stdio",
      "command": "/absolute/path/to/rotero/target/release/rotero-mcp",
      "args": [],
      "env": {
        "PDFIUM_DYNAMIC_LIB_PATH": "/absolute/path/to/rotero/lib"
      }
    }
  }
}
```

`PDFIUM_DYNAMIC_LIB_PATH` points at the PDFium library and is what enables PDF
text extraction. Leave it out and the server still starts — you just lose that
one tool.

### Finding the database

The server looks for your library in this order:

1. The `--db-path` argument
2. The `ROTERO_DB_PATH` environment variable
3. The [platform default location]({base}/docs/data-locations)

## What it exposes

Thirty-two tools, grouped by what they do:

| Purpose | Tools |
| --- | --- |
| Search | `search_papers`, `search_online`, `find_pdf` |
| Reading | `get_paper`, `list_papers`, `get_paper_annotations`, `get_paper_notes`, `list_collections`, `list_tags`, `get_papers_in_collection`, `get_papers_by_tag`, `extract_pdf_text` |
| Writing papers | `add_paper`, `update_paper`, `delete_paper`, `set_paper_read`, `set_paper_favorite`, `download_pdf` |
| Organizing | `add_tag_to_paper`, `remove_tag_from_paper`, `rename_tag`, `delete_tag`, `create_collection`, `add_paper_to_collection`, `remove_paper_from_collection`, `delete_collection`, `rename_collection` |
| Notes | `add_note`, `update_note`, `delete_note` |
| Graph | `get_paper_relationships`, `get_library_graph` |

One resource, `rotero://library/stats`, reports totals for papers, collections,
and tags plus your unread and favorite counts.

Two prompts ship with the server:

| Prompt | Argument | What it does |
| --- | --- | --- |
| `summarize-paper` | `paper_id` | Summarizes one paper from your library |
| `literature-review` | `topic` | Drafts a review across the papers on a topic |

<Callout type="warning">

The write tools are real writes. An agent with this server connected can delete
papers, collections, and tags from your library. Read
[Sync]({base}/docs/sync) before pointing an agent at a library you have not
backed up.

</Callout>
