---
title: "Concepts"
---
Kioku is built around three core concepts: **knowledge**, **sessions**, and **meetings**.

## Knowledge

Knowledge is the searchable corpus that Kioku builds from your data. It lives in two forms:

- **Documents** — PDFs you upload. Text is extracted, chunked, embedded via Ollama, and stored in Qdrant.
- **Meetings** — transcripts from recorded meetings. Each transcript segment is embedded and indexed.

All knowledge is searchable via vector similarity: `POST /knowledge/search` with a query returns the most relevant chunks ranked by semantic score.

<Note>
  Kioku uses `nomic-embed-text-v2-moe` for embeddings — a model that matches OpenAI's `text-embedding-3-small` on benchmarks while running locally on your hardware.
</Note>

## Sessions

Sessions are conversation containers. Each session has:

- **Messages** — user and assistant messages in OpenAI chat format (multi-part content)
- **Traces** — execution records that track what happened during message processing
- **Mode** — the session's purpose (e.g. "research", "chat")

Sessions are the primary way to interact with Kioku's API programmatically.

## Meetings

Meetings are ingested from Vexa's bot platform. The lifecycle:

1. A bot is requested via `POST /vexa/bots`
2. The bot joins the meeting (Google Meet, Zoom, or MS Teams)
3. Audio is captured and transcribed in real-time via Whisper
4. When the meeting ends, the transcript is sent to Hivemind
5. Transcript segments are embedded and become searchable knowledge

## MCP

The Model Context Protocol (MCP) server exposes Kioku's knowledge as tools that AI clients can call:

- `kioku_search` — search the knowledge base
- `kioku_list_meetings` — list all meetings
- `kioku_get_transcript` — get a meeting's transcript
- `kioku_list_documents` — list uploaded documents

This lets Claude, Cursor, and other MCP-compatible clients directly access your meeting context.

## Companies and Members

Kioku supports multi-tenant organization:

- **Admin** — creates the company, manages members and invites
- **Members** — invited by admin, can use sessions/knowledge/search
- **Personal** — standalone accounts without a company

CLI auth keys are long-lived tokens for terminal access.