---
layout: docs
title: Wiki
description: Concept pages and sourced claims the agent maintains beside your papers.
---

<script>
  import { base } from '$app/paths';
  import Callout from '$lib/components/docs/Callout.svelte';
</script>

The wiki is the layer a bibliography cannot store. Papers, authors, tags,
collections, notes, and citation links stay what they are. On top of them,
Rotero keeps two kinds of pages the agent can read and update:

- A **concept** is a method, dataset, benchmark, task, or idea — Attention,
  ImageNet, MMLU — with a short description and the papers it rests on.
- A **claim** is one sentence a paper states, stored with the quotation and
  the page it came from.

Open a paper and the detail panel lists the claims filed for it. Each claim
names the concepts it is about. Click a concept to read that page, the claims
that point at it, and the concepts related to it. **Back to paper** returns
to the paper you had selected.

## Add to wiki

**Add to wiki** asks the agent to file claims for the paper you are looking at.
It reads the abstract and your highlights, reuses concept pages that already
exist, and writes quotations rather than paraphrases it cannot point at. The
button waits while a reply is in progress.

Your notes, highlights, and bibliographic fields stay yours. The agent has
separate tools for those, and compiling a paper does not use them.

<Callout type="tip">

A claim filed from a chat answer without a quotation is kept, and it ranks
below a claim that quotes the paper. The next session can see it, and it does
not replace the quoted one.

</Callout>

## What the agent can call

The same pages are available to any MCP client. The catalog is
`list_concepts`. `search_wiki` searches concepts, claims, and papers.
`read_concept` opens one page. `upsert_concept` creates or merges a page.
`list_claims` and `file_claim` read and write claims. `link_claim_concept`,
`link_claims`, and `link_concepts` draw the edges. `list_stubs` shows citation
targets a PDF named that are not in the library yet.

The prompt `compile-paper` is the same request as **Add to wiki**.

Unresolved citations are recorded when Rotero scans a PDF, not by the agent.
Importing the cited paper turns the stub into an ordinary citation link.
