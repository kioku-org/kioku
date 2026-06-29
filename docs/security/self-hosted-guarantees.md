---
title: "Self-Hosted Guarantees"
description: "What changes about your data when you self-host Kioku."
---

## What Self-Hosting Means

When you run Kioku on your own infrastructure:

- **No audio leaves your machine** — faster-whisper transcribes inside your bot container
- **No text leaves your machine for embeddings** — Ollama runs locally, no OpenAI embedding API
- **No meeting data sent to Kioku** — your PostgreSQL and Qdrant hold all records
- **Kioku has zero access** to your meetings, transcripts, documents, or search queries

## Hosted vs. Self-Hosted Comparison

| Aspect | kioku.chat (hosted) | Self-hosted |
|---|---|---|
| Audio transcription | Kioku's GPU servers | Your hardware |
| Embeddings | Kioku's Ollama instance | Your Ollama instance |
| Meeting storage | Kioku's database | Your PostgreSQL |
| Vector storage | Kioku's Qdrant | Your Qdrant |
| Who can access your data | Kioku staff (via platform) | Only you (OS-level access) |
| Uptime SLA | Platform-managed | Your responsibility |
| Backups | Platform-managed | Your responsibility |

## External Calls You Control

Self-hosted Kioku makes no external calls by default. You opt in to external services:

| Service | Env var | What it enables |
|---|---|---|
| OpenAI | `OPENAI_API_KEY` | LLM for chat sessions |
| Anthropic | `ANTHROPIC_API_KEY` | Claude for chat sessions |
| RunPod | `RUNPOD_API_KEY` | Remote GPU bot pods |
| Cloudflare | `cloudflared` binary | Public tunnel for your server |

If you set `OPENAI_API_KEY`, session messages are sent to OpenAI. If you don't want that, use Anthropic or leave both unset (chat sessions will be disabled, but search and transcription still work).

## What's Not Isolated by Self-Hosting

- **DNS** — you still use Cloudflare DNS if you're using their tunnel
- **Container images** — pulled from GHCR (`ghcr.io/kioku-org/...`); inspect the Dockerfiles and build from source if you need full supply-chain control
- **Update check** — the CLI's `kioku upgrade-check` calls the GitHub releases API; no personal data sent, but network call occurs

## Encryption at Rest

Kioku does **not** encrypt data at rest in volumes. To protect data at rest:

1. Use OS-level disk encryption (LUKS on Linux)
2. Or use an encrypted cloud volume
3. Or run PostgreSQL with TDE (Transparent Data Encryption) — not configured out of the box

Application-level field encryption is applied to some sensitive fields in PostgreSQL using `HIVEMIND_ENCRYPTION_SECRET`. The scope of this is limited — most content (transcripts, document text in Qdrant) is stored plaintext.

## Audit and Compliance

For regulated environments (HIPAA, GDPR, SOC 2):

- Enable PostgreSQL audit logging
- Set up encrypted backups with offsite storage
- Restrict Docker socket access (`DOCKER_GID` to a trusted group only)
- Use a secrets manager (Vault, AWS Secrets Manager) instead of `.env` files
- Enable TLS for all inter-service communication (not currently on by default for internal services)
