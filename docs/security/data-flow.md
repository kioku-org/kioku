---
title: "Data Flow"
description: "How meeting audio and transcripts move through Kioku."
---

Understanding where data flows helps you assess privacy risk and configure your deployment accordingly.

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

## Where Your Audio Goes

1. **Captured** inside the `kioku-stateless` bot container — audio never leaves the container as raw PCM
2. **Transcribed** locally by faster-whisper running inside the same container
3. **Segments streamed** as text over Redis to meeting-api (still on your server)
4. **Never sent to a third-party ASR API** — Whisper runs on your hardware

If you use the **hosted** kioku.chat service, audio is transcribed on Kioku's GPU infrastructure. See [Self-Hosted Guarantees](/security/self-hosted-guarantees) for what changes when you self-host.

## Where Embeddings Are Computed

`nomic-embed-text-v2-moe` runs locally via Ollama. Transcript text is embedded on your machine and stored in your local Qdrant instance. No text leaves your server for the embedding step.

## External Services

By default, Kioku calls no external APIs. External calls are only made if you explicitly configure:

| Service | Trigger | Data sent |
|---|---|---|
| OpenAI API | `OPENAI_API_KEY` set | Session messages to chat completions API |
| Anthropic API | `ANTHROPIC_API_KEY` set | Session messages to Messages API |
| RunPod API | `RUNPOD_API_KEY` set | Bot spawn/stop requests (no audio or transcript) |
| Cloudflare Tunnel | `cloudflared` running | Encrypted tunnel traffic (no plaintext) |

## Data at Rest

| Data | Location | Encryption |
|---|---|---|
| Transcripts + meetings | PostgreSQL (`kioku-postgres-data` volume) | Plaintext (disk-level encryption is your responsibility) |
| Vector embeddings | Qdrant (`kioku-qdrant-data` volume) | Plaintext |
| Meeting recordings | MinIO (`kioku-minio-data` volume) | Plaintext |
| Bot session cookies | Cookie service (`kioku-cookie-data` volume) | Plaintext |
| Model weights | Ollama / Whisper volumes | N/A (public models) |
