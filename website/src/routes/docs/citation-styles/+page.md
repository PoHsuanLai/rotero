---
layout: docs
title: Citation styles
description: Generate a formatted citation for any paper in one of 14 styles, copy it to the clipboard, and set the citation key Rotero uses for BibTeX.
---

<script>
  import { base } from '$app/paths';
  import Callout from '$lib/components/docs/Callout.svelte';
  import Figure from '$lib/components/docs/Figure.svelte';
</script>

Select a paper and click **Cite** in the detail panel. The **Generate Citation**
dialog opens with a **Citation Style** dropdown, a preview of the formatted
citation, and **Copy to Clipboard**.

The preview updates as you change the style, so you can check a couple of
formats before copying. The copy button confirms with **Copied!**.

<Figure src="citation-dialog.png" alt="The Generate Citation dialog showing a style dropdown and a formatted citation preview." caption="The Generate Citation dialog. The preview reflects the selected style immediately." />

## The 14 styles

Rotero ships 14 styles:

APA 7th, Chicago Author-Date, Chicago Notes, Harvard Cite Them Right,
Vancouver, MLA 9th, Nature, ACM, ACS, AMA, AIP, APS, Springer Basic
Author-Date, and Elsevier Harvard.

**Vancouver** is the NLM/Vancouver style used across biomedical publishing.
The two Chicago entries are the author-date and notes-bibliography systems,
which differ in more than formatting — pick the one your target venue specifies.

<Callout type="note">

IEEE is not among the 14. If a venue requires it, export the entry as BibTeX
and let your LaTeX setup format it — see
[BibTeX and other formats]({base}/docs/bibtex).

</Callout>

## Citation keys

Every paper gets a citation key generated from its author, year, and title —
the handle you type in a `\cite{}`.

The key is shown in the paper detail panel with a copy button, and it is
editable per paper. Change it if it collides with another entry, or if you are
matching keys that already exist in a manuscript.

The key you set is what Rotero writes into exported `.bib` files, including the
continuously synced auto-export file, so editing it here keeps your LaTeX
citations working.

## Citing in Word

For citing while you write in Word rather than copying strings by hand, use the
[Word add-in]({base}/docs/word-addin).
