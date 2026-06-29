---
title: "For MCP / Cursor / Claude"
description: "Give Claude, Cursor, and other AI clients direct access to your meeting knowledge."
---

Kioku runs two MCP servers that AI clients can connect to:

| Server | Endpoint | Purpose |
|---|---|---|
| **Knowledge MCP** | `api.kioku.chat/mcp` | Search transcripts and documents, list meetings |
| **Meetings MCP** | `mcp.kioku.chat/mcp` | Request bots, read live transcripts, manage recordings |

## Quickest Setup (CLI)

```bash
kioku mcp
```

This prints a ready-to-paste JSON config block with both servers and your current auth token. Paste it into your client's MCP config file.

## Claude Desktop

Add to `~/Library/Application Support/Claude/claude_desktop_config.json` (macOS) or `%APPDATA%/Claude/claude_desktop_config.json` (Windows):

```json
{
  "mcpServers": {
    "Kioku": {
      "url": "https://api.kioku.chat/mcp",
      "headers": {
        "Authorization": "Bearer YOUR_HIVEMIND_JWT"
      }
    },
    "Kioku Meetings": {
      "url": "https://mcp.kioku.chat/mcp",
      "headers": {
        "Authorization": "Bearer YOUR_VEXA_API_KEY"
      }
    }
  }
}
```

Get your **Hivemind JWT** from `kioku auth-token` (after `kioku signin`).  
Get your **Vexa API key** from the dashboard under **Settings → API Keys**.

## Claude Code

Add to your project's `.claude/mcp.json` or run `kioku mcp` and paste the output.

## Cursor

Add to `.cursor/mcp.json` in your project root:

```json
{
  "mcpServers": {
    "Kioku": {
      "url": "https://api.kioku.chat/mcp",
      "headers": { "Authorization": "Bearer YOUR_JWT" }
    }
  }
}
```

## Self-Hosted Local Setup

Replace the hosted URLs with localhost:

```json
{
  "mcpServers": {
    "Kioku": {
      "url": "http://localhost:9100/mcp",
      "headers": { "Authorization": "Bearer YOUR_JWT" }
    },
    "Kioku Meetings": {
      "url": "http://localhost:18888/mcp",
      "headers": { "Authorization": "Bearer YOUR_VEXA_API_KEY" }
    }
  }
}
```

## What You Can Do

Once connected, ask your AI client:

- *"What did we decide about the deployment strategy last week?"*  
  → Claude calls `kioku_search` and returns relevant transcript excerpts.

- *"Summarize my last three standups."*  
  → Claude calls `kioku_list_meetings`, then `kioku_get_transcript` for each.

- *"Find everything we discussed about RunPod."*  
  → Semantic search across all meetings and uploaded documents.

- *"Upload this transcript."*  
  → Claude calls `kioku_ingest_meeting` with the structured data.

See [MCP Tools](/api-cli-mcp#mcp-tools) for the full list of available tools.
