---
title: "MCP Integration"
---
Connect AI clients to your Kioku knowledge base and meeting tools via the Model Context Protocol.

## What is MCP?

The [Model Context Protocol](https://modelcontextprotocol.io) (MCP) is an open standard that lets AI applications call external tools. Kioku runs **one** MCP server — `services/mcp`, a Rust binary called `kioku-mcp` — exposing all 25 tools (knowledge and meetings alike) behind a single Streamable HTTP endpoint.

<Note>
  Earlier revisions of this service were split into a Python meeting-MCP and a
  Hivemind-embedded knowledge MCP. Both were consolidated into this one Rust binary —
  Hivemind itself no longer runs any MCP server.
</Note>

| | |
|---|---|
| Endpoint | `/mcp` |
| Port | `18888` (env `PORT`) |
| Transport | Streamable HTTP (`rmcp`), stateful sessions, 30s SSE keepalive |
| Hosted URL | `mcp.kioku.chat/mcp` |
| Local URL | `http://localhost:18888/mcp` |

`kioku-mcp` doesn't hold its own database connection — it's a thin proxy in front of two
backends: the Hivemind API (for knowledge tools) and the Vexa API gateway (for meeting/bot
tools).

## Quick Setup

Run `kioku mcp` after signing in — it prints a ready-to-paste config JSON:

```bash
kioku mcp
```

<Note>
  `kioku mcp`'s output currently includes a second, stale `Kioku` entry pointed at
  `{hivemind}/mcp`, left over from the pre-consolidation architecture. Use the `Kioku
  Meetings` entry (or the unified URL above directly) — it now serves every tool.
</Note>

## Manual Configuration

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

### Local Development

```json
{
    "mcpServers": {
        "Kioku": {
            "url": "http://localhost:18888/mcp",
            "headers": {
                "Authorization": "Bearer YOUR_KIOKU_TOKEN"
            }
        }
    }
}
```

## How It Works

1. An AI client (Claude, Cursor) connects to `/mcp`.
2. It authenticates with `Authorization: Bearer <token>` (or an `x-api-key` header) —
   any Kioku credential works: a Hivemind JWT, a `kioku_...` API key, or a raw Vexa API key.
3. **Knowledge tools** (`search`, `meetings`, `transcript`, `documents`, ...) forward that
   token as-is to the Hivemind API, which validates it itself.
4. **Meeting/bot tools** (`request_meeting_bot`, `stop_bot`, `get_meeting_bundle`, ...)
   first call `GET /vexa/token` on Hivemind to exchange the token for the caller's own
   per-user Vexa API key, then forward to the Vexa gateway as `X-API-Key`. If that exchange
   fails, the original token is used as a fallback rather than hard-failing.
5. Results are returned as structured data the AI can reason about.

See [MCP Tools](/mcp/tools) for the full tool list, and
[Vexa ↔ Hivemind credential linking](/architecture/vexa-hivemind-credentials) for how the
token exchange works under the hood.

## Prompts

The server also registers 4 MCP prompts to help clients drive multi-step workflows:

| Prompt | Purpose |
|---|---|
| `vexa.meeting_prep` | Parse a meeting link, request a bot, attach prep notes |
| `vexa.during_meeting` | Check bot status + a live transcript snapshot |
| `vexa.post_meeting` | Fetch the meeting bundle and produce a summary/decisions/action items |
| `vexa.teams_link_help` | Troubleshooting checklist for unsupported Teams meetup-join links |

<Note>
  Both MCP sessions are scoped to your workspace/company context in Hivemind and Vexa
  respectively. `kioku mcp` outputs config using your current auth token.
</Note>
