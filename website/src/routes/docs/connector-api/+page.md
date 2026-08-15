---
layout: docs
title: Connector API
description: The local HTTP API on 127.0.0.1:21984 that the browser extension and Word add-in use, documented for anyone building against it.
---

<script>
  import { base } from '$app/paths';
  import Callout from '$lib/components/docs/Callout.svelte';
</script>

Rotero runs a local HTTP server on `127.0.0.1:21984` while the app is open. The
[browser extension]({base}/docs/browser-extension) and the
[Word add-in]({base}/docs/word-addin) are both clients of it, and so can
anything else you write.

It binds to `127.0.0.1` only — it is not reachable from other machines on your
network. Turn it off or change the port in
[Settings ▸ Connector]({base}/docs/settings#connector).

## Status and library

| Endpoint | Returns |
| --- | --- |
| `GET /api/status` | `{status, version, name}` |
| `GET /api/collections` | `{collections: [{id, name, parent_id?}]}` |
| `GET /api/tags` | `{tags: [{id, name, color}]}` |

`GET /api/status` is the connectivity check — that is what drives the green dot
in the extension popup.

Collections are a tree: `parent_id` is absent for top-level collections.

## Saving papers

### `POST /api/save`

Creates a paper. Every body field is optional, so send what you have.

| Field | Type |
| --- | --- |
| `url` | string |
| `doi` | string |
| `title` | string |
| `item_type` | string |
| `authors` | string[] |
| `pdf_url` | string |
| `journal` | string |
| `year` | number |
| `volume` | string |
| `issue` | string |
| `pages` | string |
| `publisher` | string |
| `abstract_text` | string |
| `collection_id` | string |
| `tag_ids` | string[] |

Pass `pdf_url` and Rotero downloads the PDF and attaches it. Pass
`collection_id` and `tag_ids` to file the paper as you create it.

### `POST /api/scrape`

Runs Rotero's translators over a page and returns what they extracted. Body:
`{url, html?, raw_html?}`. Send `url` alone to have Rotero fetch the page, or
include the HTML when you already have it — which is what the extension does,
since it is already on the page and past any login wall.

### `POST /api/scrape/continue`

Resumes a multi-step scrape.

<Callout type="warning">

This endpoint only exists in builds compiled with the `translator-engine`
feature, which is off by default. Release builds do not have it — check for a
404 rather than assuming it is there.

</Callout>

## Citations

| Endpoint | Body / query | Returns |
| --- | --- | --- |
| `GET /api/cite/styles` | — | `{styles: [{id, name}]}` |
| `GET /api/cite/search` | `?q=` | `{papers: [...]}` |
| `POST /api/cite/format` | `{paper_ids, style}` | Formatted citations |
| `POST /api/cite/bibliography` | `{paper_ids, style}` | Bibliography entries |

`style` takes an `id` from `GET /api/cite/styles`. The 14 available styles are
listed under [Citation styles]({base}/docs/citation-styles).

## Word add-in assets

The Word add-in's front end is compiled into the Rotero binary and served from
the same port:

- `/word/taskpane.html`
- `/word/taskpane.js`
- `/word/taskpane.css`
- `/word/assets/icon-16.png`, `icon-32.png`, `icon-80.png`

That is why the add-in needs Rotero running — the taskpane itself is coming from
the app.
