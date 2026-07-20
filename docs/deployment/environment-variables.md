---
title: "Environment Variables"
---
Full reference for all configuration options. See `deployment/docker/.env.example` for the
authoritative, up-to-date list.

## Database

| Variable | Default | Description |
|---|---|---|
| `DB_USER` | `kioku` | Postgres user |
| `DB_PASSWORD` | `kioku` | Postgres password |
| `DB_NAME` | `kioku` | Postgres database name |

## Hivemind

| Variable | Default | Description |
|---|---|---|
| `HIVEMIND_JWT_SECRET` | — | JWT signing secret (64-char hex, required) |
| `HIVEMIND_ENCRYPTION_SECRET` | — | Field-level encryption secret (64-char hex, required) |
| `HIVEMIND_PORT` | `9100` | Hivemind API port |

## Vexa

| Variable | Default | Description |
|---|---|---|
| `VEXA_ADMIN_API_TOKEN` | — | Vexa admin API token (required) |
| `VEXA_PUBLIC_URL` | `http://localhost:8056` | Public URL for the Vexa API gateway |
| `VEXA_PUBLIC_API_URL` | — (compose falls back to `https://meetings.kioku.chat` if unset — see warning below) | Public URL the dashboard's browser-side code uses for its WebSocket connection |
| `VEXA_BOT_IMAGE` | `ghcr.io/kioku-org/kioku-stateless:latest` | Docker image for bot pods |
| `INTERNAL_API_SECRET` | — | Secret for internal API callbacks |

<Warning>
  `docker-compose.stateful.yml` still defaults `VEXA_PUBLIC_API_URL` to
  `https://meetings.kioku.chat` at the compose level if you leave it unset. Always set it
  explicitly for self-hosted deployments, or the dashboard's browser-side WebSocket will
  silently point at Kioku's production domain instead of your server.
</Warning>

## Redis

| Variable | Default | Description |
|---|---|---|
| `REDIS_PASSWORD` | `kioku-redis` | Redis AUTH password — change from the default if Redis is exposed publicly, e.g. RunPod overflow |

## Storage

| Variable | Default | Description |
|---|---|---|
| `STORAGE_BACKEND` | `minio` | Storage backend |
| `MINIO_ACCESS_KEY` | `vexa-access-key` | MinIO access key |
| `MINIO_SECRET_KEY` | `vexa-secret-key` | MinIO secret key |
| `MINIO_BUCKET` | `vexa-recordings` | MinIO bucket name |
| `RECORDING_ENABLED` | `false` | Enable audio/video recording capture |

## Vector DB

| Variable | Default | Description |
|---|---|---|
| `QDRANT_API_KEY` | — | Qdrant API key (optional) |

Note: this deployment remaps Qdrant's default ports — HTTP/REST is `6334` and gRPC is
`6335` (not Qdrant's stock `6333`/`6334`). Hivemind connects on the **gRPC** port (`6335`).

## AI Integrations (Optional)

| Variable | Description |
|---|---|
| `OPENAI_API_KEY` | OpenAI API key (TTS, agent-api chat) |
| `OPENAI_BASE_URL` | OpenAI base URL (default: `https://api.openai.com`) |
| `ANTHROPIC_API_KEY` | Anthropic API key (agent-api chat — this is the only LLM provider agent-api reads) |
| `VEXA_TRANSCRIBER_API_KEY` | Transcription service API key |
| `ZOOM_CLIENT_ID` | Zoom OAuth client ID |
| `ZOOM_CLIENT_SECRET` | Zoom OAuth client secret |
| `TTS_API_TOKEN` | TTS service auth token |

## Transcription

Passed to each `kioku-stateless` bot pod and, if the `shared-whisper` compose profile is
enabled, the shared `kioku-whisper` service — both run the same Rust
([kiku](https://crates.io/crates/kiku)/whisper.cpp) transcription binary. See
[GPU vs CPU Modes](/deployment/gpu-cpu-modes).

| Variable | Default | Description |
|---|---|---|
| `STT_BACKEND` | `whisper` | `whisper` (local whisper.cpp, GPU with CPU fallback) \| `chirp` (Google Chirp 3 via OpenRouter) \| `gpt4o` (OpenAI gpt-4o-mini-transcribe via OpenRouter) |
| `BOT_WHISPER_MODEL` | `large-v3-turbo` | ggml model name for the local `whisper` backend (maps to the service's `MODEL_SIZE`) |
| `OPENROUTER_API_KEY` | — | Required for `chirp`/`gpt4o` backends |
| `BOT_TRANSCRIPTION_SERVICE_URL` | — | Set to `http://kioku-whisper:8000` to point local bots at the shared `kioku-whisper` instance instead of each spawning its own in-pod model. RunPod bots ignore this and always keep their in-pod service. |

## Dashboard / OAuth

| Variable | Default | Description |
|---|---|---|
| `NEXTAUTH_SECRET` | — | Dashboard session secret (required) |
| `NEXTAUTH_URL` | — | Dashboard's own public URL (required for OAuth redirects) |
| `GOOGLE_CLIENT_ID` / `GOOGLE_CLIENT_SECRET` | — | Google sign-in (also used for the CLI's Calendar-consent round trip) |
| `GITHUB_CLIENT_ID` / `GITHUB_CLIENT_SECRET` | — | GitHub sign-in |
| `AZURE_AD_CLIENT_ID` / `AZURE_AD_CLIENT_SECRET` / `AZURE_AD_TENANT_ID` | — | Microsoft Entra ID sign-in |
| `VEXA_ALLOW_DIRECT_LOGIN` | — | Allow email/password login with no OAuth configured |
| `INTERNAL_API_SECRET` | — | Shared secret for the dashboard's `/internal/provision` call into Hivemind |

## General

| Variable | Default | Description |
|---|---|---|
| `LOG_LEVEL` | `INFO` | Logging level |
| `CORS_ORIGINS` | `*` | CORS allowed origins |
| `VEXA_ENV` | `production` | Environment flag |

## Cloudflare Tunnel

| Variable | Default | Description |
|---|---|---|
| `CLOUDFLARED_CREDENTIALS_DIR` | `~/.cloudflared` | Path to Cloudflare tunnel credentials |

## RunPod

| Variable | Default | Description |
|---|---|---|
| `RUNPOD_API_KEY` | — | Local/dev fallback for the RunPod account API key. Inside a pod that is itself RunPod-hosted, RunPod auto-injects its own pod-scoped value into this name, so prefer `RUNPOD_ACCOUNT_API_KEY` there. |
| `RUNPOD_ACCOUNT_API_KEY` | — | Preferred RunPod account API key — always takes precedence over `RUNPOD_API_KEY` when set |
| `RUNPOD_GPU_TYPE` | `NVIDIA GeForce RTX 3090` | GPU type for bot pods |
| `RUNPOD_GPU_TYPES` | `NVIDIA GeForce RTX 3090,NVIDIA GeForce RTX 5090,NVIDIA RTX A5000,NVIDIA RTX A4000` | Ordered GPU fallback list for bot pods |
| `RUNPOD_CLOUD_TYPE` | `COMMUNITY` | Cloud tier (`SECURE` or `COMMUNITY`) |
| `RUNPOD_CONTAINER_DISK_GB` | `40` | Container disk size for bot pods |
| `RUNPOD_POLL_INTERVAL` | `15` | Pod status poll interval (seconds) |
| `MIN_BOT_POOL` | `0` | Idle bot pods to keep warm; `create()` claims one before cold-spawning. Only wired for RunPod deploys. |
| `BOT_IMAGE` | `ghcr.io/kioku-org/kioku-stateless:latest` | Image for bot pods |
| `BROWSER_IMAGE` | defaults to `BOT_IMAGE` | Browser/runtime image passed into the meeting profile |
| `BOT_REDIS_URL` | auto-resolved from `RUNPOD_POD_ID` when set, else defaults to `REDIS_URL` | Redis URL passed to bot pods |
| `BOT_MEETING_API_URL` | auto-resolved from `RUNPOD_POD_ID` when set, else defaults to `MEETING_API_URL` | Meeting API URL passed to bot pods |
| `BOT_TTS_URL` / `BOT_COOKIE_URL` | auto-resolved from `RUNPOD_POD_ID` when set | TTS/cookie service URLs passed to bot pods |
| `BOT_MAX_TIME_LEFT_ALONE` | `120000` (2 min, in code) | How long a bot waits alone in a meeting before self-leaving (Google Meet/Teams only — Zoom has no alone-detection) |

## Runtime API

| Variable | Default | Description |
|---|---|---|
| `ORCHESTRATOR_BACKEND` | `docker` | `docker` \| `kubernetes` \| `process` \| `runpod` |
| `USE_LOCAL_RESOURCE` | `true` | If `true`, spawn bots via the Docker socket up to `LOCAL_BOT_THRESHOLD` before overflowing to RunPod |
| `LOCAL_BOT_THRESHOLD` | `3` | Max local bots before overflow to RunPod |
| `ALLOW_PRIVATE_CALLBACKS` | `true` (hardcoded for both `runtime-api-local` and `runtime-api-runpod`) | Allows meeting-api's own `http://localhost:8080/...` callback URL |

## Build Paths

| Variable | Default | Description |
|---|---|---|
| `KIOKU_VEXA_PATH` | `../../services` | Root of the services directory (for Docker build contexts) |
| `HIVEMIND_PATH` | `../../services/hivemind` | Path to hivemind source (for Docker builds) |
| `CONTAINER_DISK_GB` | `20` | Default container disk used by `deployment/docker/scripts/runpod/deploy.sh` for the stateful pod |
| `STATEFUL_RUNPOD_CLOUD_TYPE` | `COMMUNITY` | Cloud tier used by `deployment/docker/scripts/runpod/deploy.sh` for the long-lived stateful pod |
