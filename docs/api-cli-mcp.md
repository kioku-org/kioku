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
kioku auth-token        # print stored JWT

# API key exchange
curl -X POST http://localhost:9100/auth/token \
  -H "X-API-Key: koku_your_api_key"
```

### Key Endpoints

| Method | Path | Description |
|---|---|---|
| `POST` | `/auth/register/admin` | Create company + admin user |
| `POST` | `/auth/signin` | Sign in, get JWT |
| `GET` | `/auth/me` | Current user info |
| `POST` | `/auth/token` | Exchange API key for JWT |
| `POST` | `/vexa/bots` | Spawn a meeting bot |
| `DELETE` | `/vexa/bots/<id>` | Stop a bot |
| `GET` | `/vexa/meetings` | List Vexa-recorded meetings |
| `POST` | `/meetings` | Ingest a meeting transcript |
| `GET` | `/meetings` | List all meetings |
| `POST` | `/knowledge/search` | Semantic search |
| `POST` | `/knowledge/documents` | Upload PDF |
| `GET` | `/knowledge/documents` | List documents |
| `DELETE` | `/knowledge/documents/<id>` | Delete document |
| `POST` | `/sessions` | Create chat session |
| `GET` | `/sessions` | List sessions |
| `POST` | `/sessions/<id>/messages` | Send message |
| `GET` | `/sessions/<id>/messages` | Get messages |

See individual endpoint docs in the **API** section for full request/response schemas.

---

## CLI

The `kioku` CLI is a Rust binary that wraps the Hivemind API.

### Install

```bash
cd services/cli
cargo install --path crates/cc-cli
```

### Configure

```bash
export KIOKU_SERVER=http://localhost:9100   # or api.kioku.chat for hosted

kioku signin                                # email + password
kioku auth-token                            # print JWT for MCP config
```

### Command Reference

**Auth**
```bash
kioku signin
kioku signout
kioku whoami
kioku auth-token
kioku auth-key-create
kioku auth-key-list
kioku auth-key-delete <prefix>
```

**Sessions**
```bash
kioku sessions-list
kioku sessions-create --title "Research"
kioku sessions-get <id>
kioku sessions-delete <id>
kioku send <session_id> "your question"
kioku messages <session_id>
```

**Knowledge**
```bash
kioku knowledge-search "deployment strategy"
kioku knowledge-upload ./report.pdf
kioku knowledge-documents
kioku knowledge-delete <id>
```

**Meetings**
```bash
kioku meetings-list
```

**MCP Config**
```bash
kioku mcp    # print ready-to-paste JSON config for AI clients
```

**Self-update**
```bash
kioku upgrade-check
kioku upgrade
```

---

## MCP Tools

Kioku runs two MCP servers using the streamable-HTTP transport.

### Knowledge MCP (`/mcp` on Hivemind, port 9100)

| Tool | Description |
|---|---|
| `kioku_search` | Semantic search across all meetings and documents |
| `kioku_list_meetings` | List all meetings with metadata |
| `kioku_get_transcript` | Get full transcript for a meeting |
| `kioku_list_documents` | List uploaded documents |
| `kioku_ingest_meeting` | Add a meeting transcript to the knowledge base |

**Example: search**
```json
{
  "tool": "kioku_search",
  "arguments": {
    "query": "what did we decide about the API design",
    "limit": 5
  }
}
```

**Example: get transcript**
```json
{
  "tool": "kioku_get_transcript",
  "arguments": {
    "meeting_id": "m-42"
  }
}
```

### Meetings MCP (`mcp.kioku.chat` or port 18888)

| Tool | Description |
|---|---|
| Bot request tools | Spawn and stop meeting bots |
| Transcript tools | Read live and completed transcripts |
| Recording tools | List and access recordings |

### Connecting AI Clients

Run `kioku mcp` for a ready-to-paste config block. See [MCP / Cursor / Claude](/getting-started/mcp-cursor-claude) for per-client setup.
