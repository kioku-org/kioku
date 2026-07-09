---
title: "For MCP / Cursor / Claude"
description: "Give Claude, Cursor, and other AI clients direct access to your meeting knowledge."
---

Kioku runs **one** MCP server (`kioku-mcp`) that AI clients can connect to — it exposes
both knowledge tools (search, meetings, documents, sessions) and meeting/bot tools (spawn
bots, read live transcripts, manage recordings) behind a single endpoint and a single
credential.

| | |
|---|---|
| Hosted URL | `https://mcp.kioku.chat/mcp` |
| Local URL | `http://localhost:18888/mcp` |

## Quickest Setup (CLI)

```bash
kioku mcp
```

This prints a ready-to-paste JSON config block. Paste it into your client's MCP config file.

<Note>
  The output currently includes a second, stale `Kioku` entry pointed at the Hivemind API
  URL — that one no longer serves `/mcp`. Use the `Kioku Meetings` entry (or the unified
  URL above directly); it serves every tool, knowledge and meetings alike.
</Note>

## Claude Desktop

Add to `~/Library/Application Support/Claude/claude_desktop_config.json` (macOS) or `%APPDATA%/Claude/claude_desktop_config.json` (Windows):

```json
{
  "mcpServers": {
    "Kioku": {
      "url": "https://mcp.kioku.chat/mcp",
      "headers": {
        "Authorization": "Bearer YOUR_KIOKU_TOKEN"
      }
    }
  }
}
```

Get a token via `kioku signin` then `kioku --token`. Any Kioku credential works — a
Hivemind JWT, a `kioku_...` API key, or a Vexa API key — the server exchanges internally as
needed.

## Claude Code

Add to your project's `.claude/mcp.json` or run `kioku mcp` and paste the (corrected) output.

## Cursor

Add to `.cursor/mcp.json` in your project root:

```json
{
  "mcpServers": {
    "Kioku": {
      "url": "https://mcp.kioku.chat/mcp",
      "headers": { "Authorization": "Bearer YOUR_KIOKU_TOKEN" }
    }
  }
}
```

## Self-Hosted Local Setup

Replace the hosted URL with localhost:

```json
{
  "mcpServers": {
    "Kioku": {
      "url": "http://localhost:18888/mcp",
      "headers": { "Authorization": "Bearer YOUR_KIOKU_TOKEN" }
    }
  }
}
```

## What You Can Do

Once connected, ask your AI client:

- *"What did we decide about the deployment strategy last week?"*
  → Claude calls `search` and returns relevant transcript excerpts.

- *"Summarize my last three standups."*
  → Claude calls `meetings`, then `transcript` for each.

- *"Join this meeting and prep me."*
  → Claude calls `request_meeting_bot`, and later `get_meeting_bundle` for a summary.

- *"Upload this transcript."*
  → Claude calls `meeting` with the structured data.

See [MCP Tools](/mcp/tools) for the full list of available tools.
