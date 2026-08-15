---
layout: docs
title: BibTeX and other formats
description: Import .bib, .ris, .nbib, and CSL-JSON libraries with their PDFs, export BibTeX, and keep a .bib file continuously in sync for LaTeX.
---

<script>
  import { base } from '$app/paths';
  import Callout from '$lib/components/docs/Callout.svelte';
  import Figure from '$lib/components/docs/Figure.svelte';
</script>

Rotero reads and writes the formats reference managers exchange, which is how
you move a library in from Zotero, Mendeley, or a `.bib` file you have been
maintaining by hand.

## Importing

Import accepts:

| Extension | Format |
| --- | --- |
| `.bib`, `.bibtex` | BibTeX / BibLaTeX |
| `.ris` | RIS |
| `.nbib` | PubMed NBIB |
| `.json` | CSL-JSON |

If the `.bib` file references PDFs by relative path — the layout Zotero and
Better BibTeX produce when you export with attachments — Rotero resolves those
paths and imports the PDFs alongside the entries. Keep the exported `.bib` next
to its files folder when you move it.

When the import finishes, Rotero reports what it did: `Imported 214/220 papers
(180 PDFs)`.

### Filling in the missing PDFs

Entries that arrive without a PDF trigger a prompt titled **Download Open Access
PDFs**:

> N imported papers don't have PDFs. Search OpenAlex for open access versions?

Choose **Skip** to leave them as metadata-only, or **Download** to have Rotero
search OpenAlex for legally free full text. A progress banner tracks the run and
has a **Cancel** button — canceling keeps whatever it already downloaded.

<Callout type="tip">

Large imports are worth doing in one pass with **Download**, then walking away.
The search is per paper and rate-limited by the upstream services, so a few
hundred entries takes a while.

</Callout>

## Exporting

`⌘E`, or **File ▸ Export BibTeX…**, writes your library to
`rotero-export.bib`. Entries use the citation keys shown in the paper detail
panel, so keys you have edited carry through — see
[Citation styles]({base}/docs/citation-styles).

## Auto-export for LaTeX

A one-off export goes stale the moment you add a paper. For writing in LaTeX,
point Rotero at a `.bib` file and let it maintain the file for you.

Set the path in **Settings ▸ General ▸ Import & Metadata**. Rotero then keeps
that file up to date as your library changes, in the Better BibTeX style.

Put the path inside your paper's repository and your `\bibliography{}` stays
correct without you thinking about it — add a paper in Rotero, cite it in your
next `\cite{}`, rebuild.

<Callout type="note">

Auto-export writes the whole library to the file. It is meant to be a
generated artifact — do not hand-edit it, and consider adding it to
`.gitignore` if you would rather each author generate their own.

</Callout>

## Moving in from Zotero

Export from Zotero as BibTeX with "Export Files" checked, then import the
resulting `.bib` in Rotero. Entries, metadata, and the attached PDFs come across
in one pass. Anything that arrives without a PDF can be filled in through the
open access prompt above.
