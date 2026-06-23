---
title: "Hivemind"
---
Hivemind is Kioku's core API server, built in Rust with axum.

## Responsibilities

- **Authentication** — admin/personal/member registration, JWT sessions, API key exchange
- **Company management** — members, invites, provider API keys, CLI auth keys
- **Knowledge** — PDF upload, text extraction, embedding via Ollama, vector search via Qdrant
- **Sessions** — conversation sessions with messages and traces
- **Meetings** — meeting ingest (transcript → embeddings → searchable knowledge)
- **Usage tracking** — token usage per user
- **Vexa proxy** — bot spawn requests, meeting listing
- **MCP server** — Model Context Protocol tools for AI clients

## Stack

| Technology | Purpose |
|---|---|
| Rust + axum | HTTP server |
| sqlx + Postgres (pgvector) | Relational data + migrations |
| Qdrant | Vector similarity search |
| Ollama (nomic-embed-text-v2-moe) | Local embeddings |
| AES-GCM | API key encryption at rest |
| jsonwebtoken | JWT auth tokens |
| rmcp | MCP server (streamable-http transport) |

## Source

```
services/hivemind/
├── src/
│   ├── main.rs               # entry: init embedder + Qdrant, build router, serve
│   ├── router.rs              # all route definitions
│   ├── handlers/             # HTTP handlers (auth, company, knowledge, meeting, ...)
│   ├── repos/                # Data access layer
│   ├── services/             # Business logic (crypto, embedding, knowledge, pdf, vector)
│   ├── mcp/                  # MCP server (handler, transport)
│   └── middleware.rs         # AuthContext extractor
├── migrations/               # SQL migrations
└── tests/                    # Integration tests (6 files)
```