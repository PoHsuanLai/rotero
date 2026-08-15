---
layout: docs
title: Citation graph
description: See your library as a force-directed graph linked by tags, collections, authors, journals, or citations.
---

<script>
  import { base } from '$app/paths';
  import Callout from '$lib/components/docs/Callout.svelte';
  import Figure from '$lib/components/docs/Figure.svelte';
</script>

The **Graph** button in the library header draws your library as a force-directed
graph: one node per paper, edges from whatever relationship you pick. It is a way
to see structure a list cannot show you — which authors span two topics, which
paper everything else cites.

<Figure src="graph-citations.png" alt="A force-directed graph of papers with directed citation edges between them." caption="The graph in Citations mode. Edges run from the citing paper to the cited one." />

## Edge modes

Five modes, one at a time, each drawn in its own color:

| Mode | Connects papers that |
| --- | --- |
| Tags | Share a tag |
| Collections | Sit in the same collection |
| Authors | Share an author |
| Journals | Appeared in the same journal |
| Citations | Cite one another |

**Citations** is the only directed mode — its edges run from the citing paper to
the cited one, so you can tell a foundational paper (many edges arriving) from a
survey (many leaving). Those edges come from citation links Rotero extracts out
of your PDFs, so they only appear between two papers that are both in your
library. Following a reference from inside a PDF and importing it is what grows
this view — see [Following citations]({base}/docs/citations-in-pdfs).

**Authors** and **Journals** work on any library, including one with no PDFs at
all, because they only need metadata.

## Moving around

Drag the canvas to pan and drag a node to reposition it. **Re-center** puts the
layout back in view when you have wandered off.

The **Search papers...** box filters by highlighting: matching nodes stand out
while the rest of the graph stays in place, so you keep the surrounding
structure instead of losing it to a filtered subset.

Hover a node for a tooltip identifying the paper. Click it to open that paper.

If your library is empty the graph says "No papers in library" — add papers
first, from [Adding papers]({base}/docs/importing).

<Callout type="tip">

Switch to **Tags** or **Collections** to audit your own organizing. Papers
floating unconnected are the ones you never filed, and clusters that should
touch but don't usually mean two tags that ought to be one.

</Callout>
