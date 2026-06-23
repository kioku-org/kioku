---
title: "MCP Integration"
---
Connect AI clients to your Kioku knowledge base via the Model Context Protocol.

## What is MCP?

The [Model Context Protocol](https://modelcontextprotocol.io) (MCP) is an open standard that lets AI applications call external tools. Kioku runs an MCP server with streamable-http transport at `/mcp`.

## Configuration

### Claude Desktop / Claude Code

Add to your MCP client config:

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

### Via CLI

```bash
kioku mcp
```

Prints ready-to-paste MCP server config JSON with your current auth token.

### Local Development

```json
{
    "mcpServers": {
        "Kioku": {
            "url": "http://localhost:9100/mcp",
            "headers": {
                "Authorization": "Bearer YOUR_JWT_TOKEN"
            }
        }
    }
}
```

## How It Works

1. AI client (Claude, Cursor) connects to Hivemind's `/mcp` endpoint
2. Authentication via `Authorization: Bearer <token>` header
3. Company and user context extracted from MCP session `_meta`
4. AI client can call any of the registered MCP tools
5. Results are returned as structured data the AI can reason about

<Note>
  The MCP session is scoped to your company. All tool calls operate within your company's knowledge base.
</Note>