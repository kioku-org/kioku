---
title: "Agent Context"
description: "How AI agents access and act on your knowledge base."
---

Agent context is how Kioku makes your knowledge base available to AI systems — both through a REST API for programmatic access and through MCP for direct AI client integration.

## Sessions

Sessions are conversation containers that let you build chat-style interactions against your knowledge base.

```bash
# Create a session
kioku sessions-create --title "Q3 Planning Research"

# Send a message
kioku send <session_id> "What did we discuss about the deployment strategy?"

# View message history
kioku messages <session_id>
```

Each session has:
- **Messages** — user/assistant turns in OpenAI chat format
- **Traces** — execution records (what tools were called, what was retrieved)
- **Mode** — session purpose (e.g. `research`, `chat`)

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

The preferred way to give AI clients access to Kioku. Both MCP servers are streamable-HTTP:

**Knowledge MCP** (`api.kioku.chat/mcp` or `localhost:9100/mcp`):
- `kioku_search` — semantic search across all knowledge
- `kioku_list_meetings` — list all meetings
- `kioku_get_transcript` — full transcript of a meeting
- `kioku_list_documents` — list uploaded documents
- `kioku_ingest_meeting` — add a meeting transcript

**Meetings MCP** (`mcp.kioku.chat/mcp` or `localhost:18888/mcp`):
- Bot request and management tools
- Real-time transcript access
- Recording management

See [MCP Tools](/api-cli-mcp#mcp-tools) for full tool signatures.

## Embedding Model

All knowledge is embedded with `nomic-embed-text-v2-moe` via Ollama, running locally on your hardware:

| Metric | Value |
|---|---|
| MTEB score | 63.9 (vs OpenAI text-embedding-3-small: 62.3) |
| Dimensions | 256–768 |
| GPU latency | 5–20ms |
| CPU latency | 50–200ms |

Query embeddings use the same model, so search is consistent across all knowledge types.
