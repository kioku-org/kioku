---
title: "Concepts"
---
Kioku is built around three core concepts: **knowledge**, **sessions**, and **meetings**.

## Knowledge

Knowledge is the searchable corpus that Kioku builds from your data. It lives in three forms:

- **Documents** — PDF, DOCX, PPTX, TXT, or MD files you upload. Text is extracted (with an OCR fallback for scanned PDFs), chunked, embedded via Ollama, and stored in Qdrant.
- **Meetings** — transcripts from recorded meetings. Each transcript segment is embedded and indexed.
- **Sessions** — arbitrary ingested content (e.g. a coding session summary) via `POST /knowledge/sessions`, chunked with a paragraph-aware splitter.

All knowledge is searchable via vector similarity: `POST /knowledge/search` with a query returns the most relevant chunks ranked by semantic score.

<Note>
  Kioku uses `nomic-embed-text-v2-moe` for embeddings — a model that matches OpenAI's `text-embedding-3-small` on benchmarks while running locally on your hardware.
</Note>

## Sessions

Sessions are conversation containers. Each session has:

- **Messages** — user and assistant messages in OpenAI chat format (multi-part content)
- **Traces** — execution records that track what happened during message processing
- **Mode** — the session's purpose (e.g. "research", "chat")

## Meetings

Meetings are ingested from Vexa's bot platform. The lifecycle:

1. A bot is requested via `kioku meet <link>` (or `POST /vexa/bots`)
2. The bot joins the meeting (Google Meet, Zoom, or MS Teams)
3. Audio is captured and transcribed in real-time by an embedded faster-whisper server inside the bot pod
4. When the meeting ends, the transcript is sent to Hivemind
5. Transcript segments are embedded and become searchable knowledge

## MCP

Kioku runs **one** unified [Model Context Protocol](https://modelcontextprotocol.io) server (`kioku-mcp`) that exposes both knowledge and meeting tools to AI clients — 25 tools in total, including:

- `search` — search the knowledge base
- `meetings` / `transcript` — list meetings, get a transcript
- `documents` — list uploaded documents
- `request_meeting_bot` / `stop_bot` — spawn or stop a meeting bot
- `get_meeting_bundle` — transcript + recordings + share link in one call

This lets Claude, Cursor, and other MCP-compatible clients directly access your meeting context. See [MCP overview](/mcp/overview) for the full list and auth model.

## Workspaces and Members

Kioku supports multi-tenant organization via **workspaces**:

- **Admin** — creates the workspace, manages members and invites
- **Members** — invited by an admin, can use sessions/knowledge/search
- **Personal** — standalone accounts with their own auto-created workspace
- A single user can belong to and switch between multiple workspaces (`kioku ws`)

CLI API keys are long-lived tokens for terminal/CI access, scoped to a workspace.
