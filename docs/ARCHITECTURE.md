# Kioku Architecture

> **kioku** — save your context, wherever and whenever you are.

## Overview

Kioku is a context infrastructure platform that captures, stores, and retrieves knowledge from meetings, documents, and conversations. It combines a Rust API server (Hivemind) with a meeting-bot platform (Vexa) to provide real-time meeting transcription, knowledge search, and MCP integration.

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

## Components

### Hivemind (`services/hivemind/`)

The core API server. Built in Rust with axum. Responsibilities:

- **Authentication** — admin/personal/member registration, JWT-based sessions, API key exchange
- **Company management** — members, invites, provider API keys, CLI auth keys
- **Knowledge** — PDF upload, text extraction, embedding via Ollama, vector search via Qdrant
- **Sessions** — conversation sessions with messages and traces
- **Meetings** — meeting ingest (transcript → embeddings → searchable knowledge)
- **Usage tracking** — token usage per user
- **Vexa proxy** — bot spawn requests, meeting listing
- **MCP server** — Model Context Protocol tools for AI clients (`kioku_search`, `kioku_list_meetings`, etc.)

**Stack:** Rust + axum + sqlx (Postgres) + Qdrant + Ollama (nomic-embed-text-v2-moe)

### Kioku CLI (`apps/cli/`)

Rust CLI client. 4 crates:

| Crate | Purpose |
|---|---|
| `cc-cli` | Clap command dispatcher (binary: `kioku`) |
| `cc-kioku` | HTTP client over reqwest |
| `cc-auth` | Auth file management (`~/.config/kioku/auth.json`) |
| `cc-upgrade` | Self-update via GitHub releases |

Commands: `signin`, `whoami`, `sessions-*`, `send`, `knowledge-*`, `meetings-list`, `mcp`, `upgrade`, and more.

### Vexa (`services/vexa/`)

Vendored [Vexa](https://github.com/Vexa-ai/vexa) meeting-bot platform. 13 services:

| Service | Port | Purpose |
|---|---|---|
| api-gateway | 8000 | Public API entry point |
| admin-api | 8001 | Admin operations |
| meeting-api | 8080 | Bot lifecycle, meeting records |
| agent-api | 8100 | AI agent integration |
| runtime-api | 8090 | Container orchestration (Docker/K8s/Process/RunPod) |
| transcription-service | 80 | Whisper speech-to-text |
| tts-service | 8002 | Text-to-speech |
| mcp | 18888 | Vexa MCP server |
| redis | 6379 | Transcription streams, scheduling |
| minio | 9000 | Recording storage |
| vexa-bot | — | Playwright browser bot (joins meetings) |

### Deployment (`deployment/`)

**Docker Compose** (`deployment/docker/`):
- `docker-compose.stateful.yml` — Postgres (pgvector) + Qdrant
- `docker-compose.stateless.yml` — All app services + Ollama + Cloudflared
- Scripts: `setup.sh`, `manage.sh`, `healthcheck.sh`, `smoke-test.sh`

**RunPod** (`deployment/runpod/`):
- `Dockerfile.stateful` — single CPU pod with all always-on services (supervisord)
- `Dockerfile.stateless` — GPU pod with bot + Whisper (ephemeral, per meeting)
- Runtime-api spawns/kills bot pods via RunPod REST API

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

1. AI client (Claude, Cursor, etc.) connects to Hivemind MCP endpoint
2. MCP tools available: `kioku_search`, `kioku_list_meetings`, `kioku_get_transcript`, etc.
3. AI client can search knowledge, list meetings, get transcripts — all through the authenticated MCP session