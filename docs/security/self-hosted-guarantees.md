---
title: "Self-hosted guarantees"
description: "What you control, and what external services can still receive data."
---

## What self-hosting gives you

When you run `kioku-stateful` on infrastructure you control, you operate the API, dashboard,
database, vector index, object storage, and their persistent volumes. You choose who has
network and operating-system access to them.

With local bot execution, meeting bots and transcription workloads also run on your Docker
host. The default embedding stack is local Ollama plus Qdrant, so document and transcript
text does not need an external embedding provider.

## What self-hosting does not guarantee

Self-hosting does not make every deployment fully isolated. Data can leave your
infrastructure when you opt into a remote service or expose an endpoint publicly.

## External calls you control

You opt into the external services below. Review their data handling terms before enabling
them.

| Service | Env var | What it enables |
|---|---|---|
| Dashboard AI chat (any `AI_MODEL`-selected provider — OpenAI, Anthropic, Azure OpenAI, OpenRouter, etc.) | `AI_MODEL` + `AI_API_KEY` | LLM for the dashboard's chat feature |
| Anthropic | `ANTHROPIC_API_KEY` | agent-api's in-meeting AI agent (Claude Code CLI) — the only LLM provider agent-api reads |
| OpenRouter | `OPENROUTER_API_KEY` | Cloud speech-to-text (`STT_BACKEND=chirp`/`gpt4o`) — sends raw meeting audio off your infrastructure instead of using local whisper.cpp |
| RunPod | `RUNPOD_API_KEY` or `RUNPOD_ACCOUNT_API_KEY` | Remote bot execution; capture and transcription data traverse RunPod infrastructure |
| Cloudflare | `cloudflared` binary | Public tunnel for your server |

If you set `AI_API_KEY`/`AI_MODEL`, `ANTHROPIC_API_KEY`, or `OPENROUTER_API_KEY`, the
associated feature can send its request content (chat messages, agent session content, or
raw meeting audio, respectively) to that provider. Public dashboard, API, and MCP endpoints
also require TLS, strong credentials, and network access controls.

## Other external dependencies

- **Cloudflare Tunnel** — public traffic traverses Cloudflare when you enable its tunnel
- **Container images** — pulled from GHCR (`ghcr.io/kioku-org/...`); inspect the Dockerfiles and build from source if you need full supply-chain control
- **Update check** — the CLI's `kioku upgrade` calls the GitHub releases API; no personal data sent, but network call occurs

## Encryption at Rest

Kioku does **not** encrypt data at rest in volumes. To protect data at rest:

1. Use OS-level disk encryption (LUKS on Linux)
2. Or use an encrypted cloud volume
3. Or run PostgreSQL with TDE (Transparent Data Encryption) — not configured out of the box

Kioku does not apply application-level field encryption today. `HIVEMIND_ENCRYPTION_SECRET`
is required at deploy time, but the Hivemind service has no code that actually reads or
uses it — passwords and API keys are bcrypt-hashed (one-way, not encryption), and
everything else (transcripts, document text, session messages) is stored plaintext in
PostgreSQL and Qdrant.

## Audit and Compliance

For regulated environments (HIPAA, GDPR, SOC 2):

- Enable PostgreSQL audit logging
- Set up encrypted backups with offsite storage
- Restrict Docker socket access (`DOCKER_GID` to a trusted group only)
- Use a secrets manager (Vault, AWS Secrets Manager) instead of `.env` files
- Enable TLS for all inter-service communication (not currently on by default for internal services)
