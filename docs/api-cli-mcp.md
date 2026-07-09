---
title: "API / CLI / MCP"
description: "Reference for the REST API, CLI, and MCP tools."
---

## REST API

All REST endpoints are served by the **Hivemind API** on port 9100 (or `api.kioku.chat` on hosted).

### Authentication

All endpoints except `/health` require a JWT bearer token:

```
Authorization: Bearer <token>
```

Get a token via:
```bash
# CLI
kioku signin
kioku --token        # print stored JWT

# API key exchange
curl -X POST http://localhost:9100/auth/token \
  -H "X-API-Key: kioku_your_api_key"
```

A user can belong to multiple workspaces; send `X-Workspace-Id: <id>` to operate on a
non-default workspace (must be one of the token's memberships), otherwise the token's
default workspace is used.

### Key Endpoints

| Method | Path | Description |
|---|---|---|
| `POST` | `/auth/register/admin` | Create workspace + admin user |
| `POST` | `/auth/register/personal` | Create a standalone user + auto-named workspace |
| `POST` | `/auth/register/member` | Join a workspace via invite |
| `POST` | `/auth/signin` | Sign in, get JWT |
| `GET` | `/auth/me` | Current user info |
| `POST` | `/auth/token` | Exchange API key for JWT |
| `GET` | `/workspaces` | List every workspace the token belongs to |
| `POST` | `/workspaces` | Create an additional workspace |
| `GET` / `PUT` | `/workspace/config` | Get/update the active workspace's config |
| `GET` | `/workspace/members` | List members |
| `GET` / `POST` | `/workspace/invites` | List/create invites |
| `GET` / `POST` | `/workspace/auth-keys` | List/create long-lived API keys |
| `POST` | `/vexa/bots` | Spawn a meeting bot |
| `GET` | `/vexa/bots/status` | List running bots |
| `DELETE` | `/vexa/bots/:platform/:native_meeting_id` | Stop a bot |
| `GET` | `/vexa/meetings` | List Vexa-recorded meetings |
| `GET` | `/vexa/token` | Resolve the caller's per-user Vexa API key |
| `POST` | `/meetings` | Ingest a meeting transcript |
| `GET` | `/meetings` | List all meetings |
| `GET` | `/meetings/:id/transcript` | Get a meeting's transcript |
| `POST` | `/knowledge/search` | Semantic search |
| `POST` | `/knowledge/documents` | Upload a document (PDF/DOCX/PPTX/TXT/MD) |
| `GET` | `/knowledge/documents` | List documents |
| `DELETE` | `/knowledge/documents/:id` | Delete a document |
| `POST` | `/knowledge/sessions` | Ingest arbitrary content (e.g. a coding session) |
| `POST` | `/sessions` | Create chat session |
| `GET` | `/sessions` | List sessions |
| `POST` | `/sessions/:id/messages` | Send message |
| `GET` | `/sessions/:id/messages` | Get messages |

See individual endpoint docs in the **API** section for full request/response schemas.

---

## CLI

The `kioku` CLI is a Rust binary that wraps Hivemind, Vexa, and Google Calendar.

### Install

```bash
cd services/cli
cargo install --path crates/cc-cli
```

### Configure

```bash
export KIOKU_SERVER=http://localhost:9100   # or api.kioku.chat for hosted

kioku signin      # opens a browser: pick Google or GitHub
kioku --token     # print JWT for MCP config
```

### Command Reference

**Auth**
```bash
kioku signin
kioku signin --api-key <key>
kioku signout
kioku whoami
```

**Knowledge**
```bash
kioku search "deployment strategy" --limit 10
kioku docs
kioku docs ./report.pdf
kioku docs --delete <id>
```

**Meetings**
```bash
kioku meet
kioku meet <link>
kioku meet --kill <bot-id>
kioku meet --transcript <meeting-id>
```

**Calendar**
```bash
kioku cal
kioku cal --week
kioku cal --date DD/MM/YYYY
```

**API keys**
```bash
kioku keys
kioku keys --create --name ci
kioku keys --delete <id-or-prefix>
```

**Workspaces**
```bash
kioku ws
kioku ws <name>
kioku ws --create "New Team"
```

**Teammates**
```bash
kioku invite
kioku invite <email>
kioku invite --revoke <id>
```

**MCP config**
```bash
kioku mcp    # print ready-to-paste JSON config for AI clients
```

**Self-update**
```bash
kioku upgrade
```

See [Kioku CLI](/architecture/cli) for the full command reference including global flags,
hidden commands, and the sign-in OAuth flow.

---

## MCP Tools

Kioku runs **one** unified MCP server (`services/mcp`, binary `kioku-mcp`) at
`/mcp` (Streamable HTTP), port **18888** — reachable at `mcp.kioku.chat/mcp` on hosted, or
via api-gateway's `/mcp` forward. It hosts both the knowledge tools and the meeting/bot
tools behind one endpoint and one credential.

### Knowledge tools (proxy to Hivemind)

| Tool | Description |
|---|---|
| `search` | Semantic search across documents, meetings, and sessions |
| `meetings` | List all meetings |
| `meeting_get` | Get a specific meeting's details |
| `transcript` | Get a meeting's full transcript |
| `documents` | List uploaded documents |
| `document_delete` | Delete a document |
| `session` | Ingest a coding/work session |
| `meeting` | Ingest a raw meeting transcript |

### Meeting/bot tools (proxy to the Vexa gateway)

| Tool | Description |
|---|---|
| `parse_meeting_link` | Parse a meeting URL into platform + native meeting id |
| `request_meeting_bot` | Spawn a bot (idempotent — returns the existing meeting on a 409) |
| `get_meeting_transcript` | Real-time transcript for a live meeting |
| `get_bot_status` | Status of currently running bots |
| `update_bot_config` | Update an active bot's config |
| `stop_bot` | Remove a bot from a meeting |
| `list_meetings` | List Vexa meetings, paginated/filterable |
| `update_meeting_data` | Update meeting metadata |
| `delete_meeting` | Purge/anonymize a finalized meeting |
| `list_recordings` / `get_recording` / `delete_recording` | Recording management |
| `get_recording_media_download` | Get a download URL for a recording's media file |
| `get_recording_config` / `update_recording_config` | Recording configuration |
| `get_meeting_bundle` | Transcript + recordings + share link in one call |
| `create_transcript_share_link` | Short-lived public transcript URL |

**Example: search**
```json
{
  "tool": "search",
  "arguments": {
    "query": "what did we decide about the API design",
    "limit": 6
  }
}
```

**Example: get transcript**
```json
{
  "tool": "transcript",
  "arguments": {
    "meeting_id": "m-42"
  }
}
```

### Auth

Send `Authorization: Bearer <token>` (or `x-api-key: <token>`) with **any** Kioku
credential — a Hivemind JWT, a `kioku_...` API key, or a raw Vexa API key. Knowledge tools
forward that token straight to Hivemind; meeting/bot tools first exchange it for the
caller's per-user Vexa key via Hivemind's `GET /vexa/token`. One credential works for
every tool. See [MCP overview](/mcp/overview) for details.

### Connecting AI Clients

Run `kioku mcp` for a ready-to-paste config block. See [MCP / Cursor / Claude](/getting-started/mcp-cursor-claude) for per-client setup.
