---
layout: docs
title: Where your data lives
description: The folder holding your database, PDFs, cache, and config — where it is, what is in it, and how to move it.
---

<script>
  import { base } from '$app/paths';
  import Callout from '$lib/components/docs/Callout.svelte';
</script>

Everything Rotero knows about your library sits in one folder on your machine.

| Platform | Location |
| --- | --- |
| macOS | `~/Library/Application Support/com.rotero.Rotero/` |
| Linux | `~/.local/share/rotero/` |

## What is in it

| Item | Contents |
| --- | --- |
| `rotero.db` | The SQLite database: papers, collections, tags, annotations, notes, and citation links |
| `pdfs/` | Rotero's own copies of the PDFs you imported |
| `cache/` | Rendered pages. Safe to delete — Rotero rebuilds it |
| `config.json` | Your settings |

## Moving it

**Settings ▸ General ▸ Library location** has a folder picker and a reset to
default. Changing it hot-swaps the database, so the app switches to the library
at the new path without a restart.

<Callout type="warning">

Picking a new location points Rotero at whatever library is there — it does not
move your existing one. Copy the folder yourself first, then point Rotero at the
copy.

</Callout>

## Backing up

Copy the whole folder while Rotero is closed. That is the backup — database,
PDFs, and settings together. Restoring is copying it back.

[Sync]({base}/docs/sync) is not a backup: it replicates deletions to your other
machines as reliably as it replicates everything else.

## What does not leave your machine

There is no account and no telemetry. Rotero does not phone home, and there is
no server holding a copy of your library.

Rotero does make network requests when you ask it to — fetching metadata from
CrossRef and the other [metadata sources]({base}/docs/importing), downloading
open-access PDFs, and checking for updates (which you can turn off in
**Settings ▸ About**). The [connector]({base}/docs/connector-api) binds to
`127.0.0.1`, so it is not reachable from other machines on your network.
