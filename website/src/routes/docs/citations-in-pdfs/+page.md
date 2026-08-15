---
layout: docs
title: Following citations
description: Click an in-text citation in a PDF to see the reference it points at, find it in your library or on the web, and import or jump to it.
---

<script>
  import { base } from '$app/paths';
  import Callout from '$lib/components/docs/Callout.svelte';
  import Figure from '$lib/components/docs/Figure.svelte';
</script>

Click any in-text citation — `[14]`, `(Vaswani et al., 2017)` — while reading a
PDF and Rotero opens a floating card showing the reference it points at, pulled
from the document's own bibliography.

This is the feature most worth knowing about, and it is easy to miss because it
has no button. The citations in the text are the interface.

<Figure src="pdf-reader.png" alt="A PDF page with a citation card floating over it, showing the resolved reference and its actions." caption="Clicking a citation opens a card with the reference text and what Rotero found for it." />

## What the card shows

Rotero first extracts the reference text from the bibliography at the end of the
document. Then it tries to resolve that reference to a real paper: your library
first, and if nothing matches, the web — OpenAlex, CrossRef, and arXiv.

The actions on the card depend on what it found:

| Action | What it does |
| --- | --- |
| Open | Opens the cited paper in your library |
| Import | Adds the paper to your library; reads **Imported** once it is in |
| Jump to | Navigates to the cited line in this document — the line, not just the page |
| Open in browser | Opens the publisher or preprint page |

Click anywhere else, or press `Esc`, to dismiss the card.

<Callout type="tip">

**Jump to** is the one to reach for when a paper cites its own earlier section
or figure. It lands on the exact line, so you can read the sentence and click
back without losing your place.

</Callout>

## Building a library by reading

Because **Import** is one click from the citation itself, the fastest way to
build out a topic is to read one good survey and import the references you
actually stop to look at. Each import lands in your library with its metadata
already filled in, and Rotero can fetch open access PDFs for the ones that have
them — see [Adding papers]({base}/docs/importing).

Papers you follow this way also feed the
[citation graph]({base}/docs/graph), which draws directed edges from citing to
cited papers once both are in your library.

## Other links in a PDF

The same link handling covers the rest of the document. Internal links — a
figure reference, a section cross-reference — jump within the file. External
DOI and arXiv links open in your browser.

<Callout type="note">

Citation cards depend on the PDF having a machine-readable bibliography.
Publisher PDFs and arXiv preprints usually do. Scanned documents and some
conference proceedings do not, and citations in them stay inert.

</Callout>
