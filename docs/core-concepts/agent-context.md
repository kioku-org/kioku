---
title: "Agent Context"
description: "How AI agents access and act on your knowledge base."
---

Agent context is how Kioku makes your knowledge base available to AI systems — both through a REST API for programmatic access and through MCP for direct AI client integration.

## Sessions

Sessions are conversation containers that let you build chat-style interactions against
your knowledge base. There's no dedicated CLI subcommand for them yet — use the REST API
directly:

```bash
curl -X POST http://localhost:9100/sessions \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"title": "Q3 Planning Research", "mode": "research"}'

curl -X POST http://localhost:9100/sessions/<id>/messages \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"role": "user", "content": [{"type": "text", "text": "What did we discuss about the deployment strategy?"}]}'
```

Each session has:
- **Messages** — user/assistant turns in OpenAI chat format
- **Traces** — execution records (what tools were called, what was retrieved)
- **Mode** — session purpose (e.g. `research`, `chat`)

See [Sessions API](/api/sessions) for the full endpoint reference.

## Knowledge Search

The core primitive. Query your full knowledge base (meetings + documents) with a single call:

```bash
curl -X POST http://localhost:9100/knowledge/search \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"query": "decisions about the API architecture", "limit": 10}'
```

Returns semantically ranked results with source attribution (speaker, meeting, timestamp, or document).

## MCP Tools

The preferred way to give AI clients access to Kioku. One streamable-HTTP MCP server
(`mcp.kioku.chat/mcp` or `localhost:18888/mcp`) hosts all 25 tools:

- `search` — semantic search across all knowledge
- `meetings` / `meeting_get` — list meetings, get one
- `transcript` — full transcript of a meeting
- `documents` / `document_delete` — manage uploaded documents
- `meeting` / `session` — ingest a transcript or arbitrary content
- `request_meeting_bot` / `stop_bot` / `get_bot_status` — bot lifecycle
- `get_meeting_bundle` / `create_transcript_share_link` — transcript + recordings + share link
- `list_recordings` / `get_recording` / `delete_recording` — recording management

See [MCP Tools](/mcp/tools) for full tool signatures.

## Embedding Model

All knowledge is embedded with `nomic-embed-text-v2-moe` via Ollama, running locally on your hardware:

| Metric | Value |
|---|---|
| MTEB score | 63.9 (vs OpenAI text-embedding-3-small: 62.3) |
| Dimensions | 256–768 |
| GPU latency | 5–20ms |
| CPU latency | 50–200ms |

Query embeddings use the same model, so search is consistent across all knowledge types.
