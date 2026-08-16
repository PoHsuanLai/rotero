---
layout: docs
title: Word add-in
description: Insert and restyle citations and bibliographies in Microsoft Word from your Rotero library.
---

<script>
  import { base } from '$app/paths';
  import Callout from '$lib/components/docs/Callout.svelte';
  import Figure from '$lib/components/docs/Figure.svelte';
  import Pin from '$lib/components/docs/Pin.svelte';
</script>

The Word add-in puts your library inside Microsoft Word: search your papers,
insert citations, and generate a bibliography that updates when you change
style.

Rotero has to be running. The taskpane is served from `127.0.0.1:21984` by the
Rotero app itself, so nothing about your library leaves your machine.

<Callout type="warning" title="You need a source checkout">

`manifest.xml` is not included in any release download. To install the add-in
you need the repository — either
[clone it]({base}/docs/install#building-from-source) or grab
[`word-addin/manifest.xml`](https://github.com/PoHsuanLai/rotero/blob/master/word-addin/manifest.xml)
from GitHub directly.

</Callout>

## Install on macOS

Copy the manifest into Word's add-in folder, then restart Word.

```sh
mkdir -p ~/Library/Containers/com.microsoft.Word/Data/Documents/wef
cp word-addin/manifest.xml ~/Library/Containers/com.microsoft.Word/Data/Documents/wef/
```

## Install on Windows

Windows loads add-ins from a trusted catalog folder rather than a fixed path.

1. Put `manifest.xml` in a folder and share that folder over the network — a
   share on your own machine is fine
2. In Word, go to **File ▸ Options ▸ Trust Center ▸ Trust Center Settings ▸
   Trusted Add-in Catalogs**
3. Add the folder's path as a catalog URL, then tick **Show in Menu**
4. Restart Word
5. Go to **Insert ▸ My Add-ins ▸ Shared Folder** and pick **Rotero**

## The Rotero group

After installing, a **Rotero** group appears in Word's **Home** tab with three
buttons: **Insert Citation**, **Bibliography**, and **Refresh**. Each opens the
taskpane on the matching view.

<Figure src="word-taskpane.png" alt="The Rotero task pane docked in Word, on the Cite view: a search box reading trace-based, two matching papers with checkboxes and the first one selected, a style dropdown set to APA 7th, and an Insert Citation button." caption="The task pane on the Cite view, with one paper selected and APA 7th chosen." width={360}>
  <Pin n={1} x={92} y={8}>View tabs</Pin>
  <Pin n={2} x={92} y={42}>Search results</Pin>
  <Pin n={3} x={92} y={75}>Citation style</Pin>
</Figure>

### Cite

Search your library from the box reading "Search papers by title, author, DOI…".
Results have checkboxes, so you can select several papers for one citation.
Choose a style from the dropdown and click **Insert Citation**.

### Bibliography

Lists the papers cited in the current document. Pick a style and click **Insert
Bibliography**. Inserting again does not add a second bibliography — it updates
the one already in the document in place.

### Refresh

"Update all citations and bibliography to a new style". Pick the style, click
**Refresh All**, and every citation and the bibliography are rewritten.

## How citations survive editing

Each citation is a Word content control carrying the paper's metadata, not plain
text. You can type around it, move it, and reformat the document, and Rotero
still knows which paper it refers to — which is what makes **Refresh All** able
to restyle a finished draft.

## Styles

Fourteen CSL styles are available, in both Rotero and the add-in:

| | |
| --- | --- |
| APA 7th | MLA 9th |
| Chicago Author-Date | Nature |
| Chicago Notes | ACM |
| Harvard Cite Them Right | ACS |
| Vancouver | AMA |
| Springer Basic Author-Date | AIP |
| Elsevier Harvard | APS |

IEEE is not among them.

See [Citation styles]({base}/docs/citation-styles) for what each one produces.
