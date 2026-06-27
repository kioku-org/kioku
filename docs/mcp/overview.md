---
title: "MCP Integration"
---
Connect AI clients to your Kioku knowledge base and meeting tools via the Model Context Protocol.

## What is MCP?

The [Model Context Protocol](https://modelcontextprotocol.io) (MCP) is an open standard that lets AI applications call external tools. Kioku runs two MCP servers:

| Server | Endpoint | Purpose |
|--------|----------|---------|
| **Knowledge MCP** | `api.kioku.chat/mcp` | Search knowledge, sessions, documents |
| **Meetings MCP** | `mcp.kioku.chat/mcp` | Bot management, transcripts, recordings |

## Quick Setup

Run `kioku mcp` after signing in — it prints a ready-to-paste config JSON with both servers and your current token:

```bash
kioku mcp
```

Paste the output into your AI client's MCP config file.

## Manual Configuration

### Knowledge MCP (Claude Desktop / Claude Code)

Provides `kioku_search`, `kioku_list_meetings`, `kioku_get_transcript`, and related tools.

```json
{
    "mcpServers": {
        "Kioku": {
            "url": "https://api.kioku.chat/mcp",
            "headers": {
                "Authorization": "Bearer YOUR_JWT_TOKEN"
            }
        }
    }
}
```

### Meetings MCP (Claude Desktop / Claude Code)

Provides tools for requesting bots, reading real-time transcripts, managing recordings, and meeting bundles.

```json
{
    "mcpServers": {
        "Kioku Meetings": {
            "url": "https://mcp.kioku.chat/mcp",
            "headers": {
                "Authorization": "Bearer YOUR_VEXA_API_KEY"
            }
        }
    }
}
```

Get your Vexa API key from the Kioku dashboard under **Settings → API Keys**.

### Local Development

```json
{
    "mcpServers": {
        "Kioku": {
            "url": "http://localhost:9100/mcp",
            "headers": {
                "Authorization": "Bearer YOUR_JWT_TOKEN"
            }
        },
        "Kioku Meetings": {
            "url": "http://localhost:18888/mcp",
            "headers": {
                "Authorization": "Bearer YOUR_VEXA_API_KEY"
            }
        }
    }
}
```

## How It Works

**Knowledge MCP** (Hivemind, port 9100):
1. AI client connects to `/mcp` on the Hivemind API
2. Auth via `Authorization: Bearer <jwt>` (same token as `kioku signin`)
3. Tools operate within your company's knowledge base

**Meetings MCP** (Kioku MCP service, port 18888):
1. AI client connects to `/mcp` on the Meetings MCP service
2. Auth via `Authorization: Bearer <vexa_api_key>`
3. Requests are proxied to the Vexa API gateway for bot and transcript operations

<Note>
  Both MCP sessions are scoped to your company. `kioku mcp` outputs config for both servers using your current auth token.
</Note>
