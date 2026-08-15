---
layout: docs
title: Sync
description: Keep two machines in step by pointing Rotero at a cloud-synced folder — no account, no server.
---

<script>
  import { base } from '$app/paths';
  import Callout from '$lib/components/docs/Callout.svelte';
  import Figure from '$lib/components/docs/Figure.svelte';
</script>

Rotero syncs through a shared folder. You point it at a directory that something
else already keeps in sync — iCloud Drive, Dropbox, Syncthing, a network share —
and Rotero writes its changes there and picks up changes other machines wrote.
There is no Rotero account and no Rotero server.

## Turning it on

Go to **Settings ▸ General ▸ Library & Sync**.

| Setting | What to do |
| --- | --- |
| Sync across devices | Turn it on |
| Method | Choose the transport |
| Sync folder | "Point to a cloud-synced folder (iCloud Drive, Dropbox, etc.)" |

<Figure src="settings-general.png" alt="Rotero's General settings tab, showing the library location and the Library & Sync group." caption="Sync lives in the Library & Sync group on the General tab." />

Set the same folder on every machine, and give the cloud client a moment to pull
the folder down before you expect a new machine to fill up.

<Callout type="warning" title="iCloud is not available in shipping builds">

There is a CloudKit transport in the source, but it is compiled out of release
builds. Every downloadable build of Rotero syncs through a shared folder only.
Putting that folder inside iCloud Drive works fine — that is a different thing
from the CloudKit transport.

</Callout>

## What gets synced

Both the change sets and the PDF files. A paper you import on your laptop shows
up on your desktop with its attached PDF, not just its metadata.

Rotero runs a sync pass every 30 seconds in the background. There is nothing to
press.

## Conflicts

Rotero uses a conflict-free merge, so two machines editing the same library do
not produce a "which version do you want?" prompt. Each change carries enough
information to be merged with the others regardless of the order it arrives in —
edit a paper offline on both machines and both edits survive.

The one thing a merge cannot fix is deletion racing an edit. If you delete a
paper on one machine while adding a note to it on the other, the delete wins.

<Callout type="tip">

Sync is not a backup. It propagates deletions faithfully, which is exactly what
you do not want from a backup. Copy the
[library folder]({base}/docs/data-locations) somewhere separate if you want one.

</Callout>
