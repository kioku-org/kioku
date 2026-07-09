---
title: "Data flow"
description: "How Kioku processes meeting, document, and search data."
---

This page describes the data paths in a self-hosted deployment. Hosted Kioku follows the
same product flow, but its services run on Kioku-managed infrastructure.

## Meeting Capture Flow

```mermaid
sequenceDiagram
    participant U as User
    participant HM as Hivemind API
    participant MA as meeting-api
    participant RA as runtime-api
    participant BOT as kioku-stateless bot
    participant WH as faster-whisper
    participant RD as Redis
    participant OL as Ollama
    participant QD as Qdrant

    U->>HM: POST /vexa/bots
    HM->>MA: proxy bot request
    MA->>RA: spawn bot
    RA->>BOT: create container
    BOT->>BOT: Playwright navigates to meeting URL
    BOT->>WH: audio frames (local to container)
    WH->>RD: transcript segments (via Redis stream)
    RD->>MA: segment consumer
    MA->>MA: transcript assembled
    BOT->>MA: exit callback
    MA->>HM: POST /meetings (transcript)
    HM->>OL: embed each segment
    OL->>QD: store vectors
    U->>HM: POST /knowledge/search
    HM->>QD: vector similarity search
    QD->>HM: results
    HM->>U: ranked excerpts
```

## Where meeting data goes

In a local Docker deployment, the bot captures audio and the configured transcription
workload processes it on infrastructure you operate. Transcript segments are assembled,
stored, and indexed for workspace search.

If you configure RunPod for overflow or remote bot execution, the corresponding bot and
transcription workload run on RunPod. Treat the meeting data required for that workload as
leaving your infrastructure. See [RunPod](/deployment/runpod).

## Where Embeddings Are Computed

`nomic-embed-text-v2-moe` runs locally via Ollama. Transcript text is embedded on your machine and stored in your local Qdrant instance. No text leaves your server for the embedding step.

## External Services

By default, Kioku calls no external APIs. External calls are only made if you explicitly configure:

| Service | Trigger | Data sent |
|---|---|---|
| OpenAI API | `OPENAI_API_KEY` set | Session messages to chat completions API |
| Anthropic API | `ANTHROPIC_API_KEY` set | Session messages to Messages API |
| RunPod API | `RUNPOD_API_KEY` set | Remote bot execution and its required capture/transcription traffic |
| Cloudflare Tunnel | `cloudflared` running | Encrypted tunnel traffic (no plaintext) |

## Data at Rest

| Data | Location | Encryption |
|---|---|---|
| Transcripts + meetings | PostgreSQL (`kioku-postgres-data` volume) | Plaintext (disk-level encryption is your responsibility) |
| Vector embeddings | Qdrant (`kioku-qdrant-data` volume) | Plaintext |
| Optional meeting recordings | MinIO (`kioku-minio-data` volume) and `kioku-recordings-data` | Plaintext |
| Bot session cookies | Cookie service (`kioku-cookie-data` volume) | Plaintext |
| Model weights | Ollama / Whisper volumes | N/A (public models) |
