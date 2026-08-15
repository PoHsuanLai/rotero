---
layout: docs
title: AI assistant
description: Chat with Claude, Gemini, Copilot, or Codex about your library from inside Rotero.
---

<script>
  import { base } from '$app/paths';
  import Callout from '$lib/components/docs/Callout.svelte';
</script>

Rotero has a chat panel that runs a coding agent against your library. Ask it
about a paper you are reading, have it search for related work, or let it tag
and file things for you. Open it with the **Chat** button in the library header.

## Providers

Four agents are supported. You bring your own — Rotero connects to the agent
you already have an account for.

| Provider | Agent |
| --- | --- |
| Claude | Anthropic Claude Code |
| Gemini | Google Gemini CLI |
| GitHub Copilot | Copilot |
| Codex | OpenAI Codex |

Rotero talks to all four over ACP, the Agent Client Protocol. If Node.js or the
agent's package is missing, Rotero downloads Node and installs the package for
you.

## The panel

The header shows which provider is connected and a live status:

| Status | Meaning |
| --- | --- |
| Ready | Connected and idle |
| Connecting… | Starting the agent |
| Thinking… | Working on your message |
| A tool name | Running that tool right now |
| Sign in required | Authenticate under Settings ▸ AI Agent |
| Error | The agent failed — the message says why |
| Not installed | The agent package is not on your machine yet |

Alongside it: **New chat**, **Past chats** (only for providers that support
listing previous sessions), and a close **x**.

The body is the message stream. Replies render markdown and LaTeX, so equations
come through readable. Above the input:

- A **Discussing: `<paper>`** badge when a paper is in context, so you know what
  "this paper" refers to
- A slash-command picker — type `/` to see what the agent offers
- A model selector, for providers that expose more than one model

The input reads "Ask about your papers… (/ for commands)". The send button turns
into a stop button while the agent is responding.

Tool calls appear as collapsible cards you can expand to see exactly what the
agent did. When the agent needs permission for something, the prompt appears
inline with the options it offered — you answer in the stream.

## Setting it up

Go to **Settings ▸ AI Agent**. Each provider gets a card with a status badge.
Pick one and click **Save & Connect**.

Authentication depends on the provider. The **Account** dropdown lists the auth
methods that provider supports, with **Sign in** or **Switch** beside it. For
methods that use an API key, there is a password-masked key field with **Save**
and **Clear**.

## How it reaches your library

The assistant reads and writes your library through the
[MCP server]({base}/docs/mcp), which Rotero runs for it automatically. That is
what gives it search, collections, tags, annotations, and notes.

<Callout type="note">

The embedded MCP server does not do PDF text extraction. The in-app assistant
can read a paper's metadata, annotations, and notes, but not the full text of
the PDF. Running the [standalone server]({base}/docs/mcp#standalone-server)
turns that tool on.

</Callout>
