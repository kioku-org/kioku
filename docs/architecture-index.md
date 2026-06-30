---
title: "Architecture"
---
How Kioku's components fit together.

```
┌─────────────────────────────────────────────────────────────┐
│                        Client Layer                          │
│  Kioku CLI (Rust)    MCP Client    HTTP API Consumer        │
└────────────┬──────────────┬─────────────┬───────────────────┘
             │              │             │
┌────────────▼──────────────▼─────────────▼───────────────────┐
│                    Hivemind API (:9100)                      │
│  Auth + Sessions + Knowledge Search + MCP Server             │
│  Embeddings (Ollama) → Vector Store (Qdrant)                 │
│  Postgres (pgvector) for relational data                    │
└────────────┬────────────────────────────────────┬───────────┘
             │                                     │
             │  POST /vexa/bots                    │ knowledge search
             ▼                                     ▼
┌──────────────────────────┐    ┌───────────────────────────────┐
│    Vexa API Gateway       │    │     Knowledge Pipeline         │
│  (:8056)                  │    │  PDF → text → embed → Qdrant   │
│  ┌─ Meeting API (8080)    │    │  Meeting transcript → embed    │
│  ┌─ Admin API (8001)      │    └───────────────────────────────┘
│  ┌─ Agent API (8100)      │
│  ┌─ Runtime API (8090)    │
│  │   └─ spawns bot pods   │
│  ┌─ MCP (18888)           │
│  ┌─ TTS Service (8002)    │
│  ┌─ Transcription (80)    │
│  ┌─ Redis (6379)          │
│  └─ MinIO (9000)          │
└──────────────────────────┘
         │ Runtime API
         ▼
┌──────────────────┐
│   Bot Pod (GPU)   │
│  Playwright +     │
│  Whisper + Xvfb   │
│  Lives per meeting│
└──────────────────┘
```

## Data Flow

### Meeting → Knowledge

1. User requests a bot via `POST /vexa/bots` (Hivemind proxies to Vexa)
2. Vexa runtime-api spawns a GPU bot pod
3. Bot joins the meeting (Google Meet/Zoom/Teams), captures audio
4. Whisper transcribes audio in real-time → Redis streams
5. Transcription collector writes to Postgres
6. Meeting completes → transcript sent to Hivemind `POST /meetings`
7. Hivemind embeds transcript chunks → Qdrant
8. Transcript becomes searchable via `POST /knowledge/search`

### Document → Knowledge

1. User uploads PDF via CLI or `POST /knowledge/documents`
2. Hivemind extracts text (pdf-extract)
3. Text chunked → embedded via Ollama → stored in Qdrant
4. Searchable via `POST /knowledge/search`

### MCP Integration

1. AI client (Claude, Cursor) connects to Hivemind MCP endpoint
2. MCP tools available: `search`, `meetings`, etc.
3. AI client can search knowledge, list meetings, get transcripts — all through authenticated MCP session