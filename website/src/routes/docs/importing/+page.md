---
layout: docs
title: Adding papers
description: Six ways to get papers into Rotero — a PDF on disk, a DOI, a bibliography file, drag and drop, web search results, and the browser extension.
---

<script>
  import { base } from '$app/paths';
  import Callout from '$lib/components/docs/Callout.svelte';
  import Figure from '$lib/components/docs/Figure.svelte';
</script>

Rotero has six import paths. They all end in the same place — a row in your
library with metadata attached — so pick whichever matches what you have in
front of you.

## A PDF on disk

Click **+ Add PDF** and choose the file. Rotero copies it into the library's
own `pdfs` folder rather than linking to it, so the original is free to move or
delete afterward.

After the copy, Rotero extracts the text and looks for a DOI in it. If it finds
one and automatic metadata fetching is enabled, it queries CrossRef and fills in
the title, authors, venue, year, and abstract. A file that arrived as
`1706.03762.pdf` becomes a real citation without you typing anything.

When there is no DOI in the text, the paper is imported anyway and keeps the
filename as its title. Fix it by editing the fields in the detail panel, or
re-add it through the DOI field below.

## A DOI

Click **+ DOI**, paste the identifier, and press **Fetch**.

Rotero resolves the metadata first, then tries to download an open-access copy
of the PDF. When a legal free copy exists you get the record and the file in one
step. When it does not, you still get a complete record — attach a PDF later, or
use **Find PDF** in the detail panel to try again.

<Callout type="tip">

If **Find PDF** comes up empty, it offers **Ask Agent**, which hands the search
to the [AI assistant]({base}/docs/ai-assistant) instead of giving up.

</Callout>

## A bibliography file

Use the **Import** button, press `⌘I`, or go to **File ▸ Import BibTeX…**.

Five formats are accepted:

| Extension | Format |
| --- | --- |
| `.bib`, `.bibtex` | BibTeX / BibLaTeX |
| `.ris` | RIS |
| `.json` | CSL-JSON |
| `.nbib` | PubMed NBIB |

If a `.bib` entry points at a PDF with a relative path, Rotero resolves that
path against the file's own location and imports the PDF too. That makes a
Zotero or JabRef export folder import cleanly in one go, files included.

When the import finishes you get a count: "Imported N/M papers (K PDFs)". N
below M means some entries were skipped — usually duplicates already in your
library.

[BibTeX and other formats]({base}/docs/bibtex) covers export and keeping a
`.bib` file in sync for LaTeX.

## Drag and drop

Drag one or more PDF files from your file manager onto the library panel. This
does the same work as **+ Add PDF**, including the DOI extraction and metadata
lookup, and it handles a whole folder's worth of files in one drop.

## From a web search

Type into the search field and results arrive in two groups: **In your library**
and **From the web**. The web group comes from OpenAlex, arXiv, and Semantic
Scholar.

Each web result has its own **Import** button, and there is an **Import All**
above the group when you want the whole set. Importing brings the metadata into
your library and downloads the PDF when an open-access copy is available.

<Figure src="search-unified.png" alt="Search results split into an In your library section and a From the web section with per-row Import buttons." caption="Web results import one at a time or all at once." />

This is the fastest way to add a paper you have only heard the title of. See
[Search]({base}/docs/search) for how the field behaves.

## From the browser

The Rotero browser extension adds papers from the page you are reading — a
publisher's article page, an arXiv abstract, a Google Scholar result. It talks
to the running app over a local connection, so Rotero has to be open.

Setup and supported sites are in
[Browser extension]({base}/docs/browser-extension).

## After the import

However a paper arrived, the next steps are the same. File it into a
[collection or tag it]({base}/docs/collections-tags), check whether it landed in
[Duplicates]({base}/docs/duplicates), or open it and start reading.

<Callout type="note">

Every path above writes to the same local database. Nothing is uploaded, and
metadata lookups are the only network traffic — turn them off in Settings if you
want imports to stay entirely offline.

</Callout>
