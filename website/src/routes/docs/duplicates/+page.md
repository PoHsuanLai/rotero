---
layout: docs
title: Duplicates
description: How Rotero groups duplicate papers by shared DOI or similar title, and how to merge them one at a time or all at once.
---

<script>
  import { base } from '$app/paths';
  import Callout from '$lib/components/docs/Callout.svelte';
</script>

Libraries accumulate the same paper twice. A preprint you saved, then the
published version. A BibTeX import that overlapped what you already had. The
**Duplicates** view in the sidebar collects them so you can resolve them in one
pass, and its count tells you whether it is worth opening.

## How papers are grouped

Duplicates appear as groups, each labeled with the reason it was formed:

**Shared DOI: …** — two or more papers carry the same DOI. This is the reliable
signal; the records really are the same work.

**Similar title** — the titles are close enough to look like the same paper
without a DOI to confirm it. This catches preprint-and-published pairs and
records imported from sources that never had a DOI. It also catches the
occasional false positive, such as a conference paper and its extended journal
version, so read these groups before acting on them.

## Merging

Each group gives you two per-paper actions and one for the group.

**Keep**, on a paper, makes that record the survivor and merges the others into
it. Use it when one entry clearly has the better metadata — the published
version with a real DOI and page numbers, rather than the preprint stub.

**Delete**, on a paper, removes just that record and leaves the rest of the
group alone. Use it when one entry is plainly junk rather than a merge
candidate.

**Merge All (Keep Best)**, at the top of the view, resolves every group at once,
keeping the best record from each. This is the fast path for a library you have
just imported a large `.bib` into.

<Callout type="warning">

**Merge All (Keep Best)** acts on every group, including the "Similar title"
ones. If you have pairs that are similar but genuinely different papers, work
through the groups individually instead.

</Callout>

## After merging

The Duplicates count drops as groups are resolved, and an empty view means
nothing is currently flagged. Merged papers keep the collections, tags, and
annotations attached to the surviving record.

## Avoiding duplicates in the first place

Most duplicates come from importing the same set twice. Two things reduce that:

Import bibliography files once and let the importer skip what it recognizes —
the "Imported N/M papers" count in
[Adding papers]({base}/docs/importing) reports the difference when entries are
skipped.

Search before you import. The search field shows **In your library** above
**From the web**, so a paper you already have is visible before you click
Import. See [Search]({base}/docs/search).
