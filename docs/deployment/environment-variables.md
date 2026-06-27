---
title: "Environment Variables"
---
Full reference for all configuration options.

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
| `HIVEMIND_PORT` | `9100` | Hivemind API port |

## Vexa

| Variable | Default | Description |
|---|---|---|
| `VEXA_ADMIN_API_TOKEN` | — | Vexa admin API token (required) |
| `VEXA_PUBLIC_URL` | `http://localhost:8056` | Public URL for Vexa API |
| `VEXA_BOT_IMAGE` | `vexa-bot:dev` | Docker image for bot containers |
| `INTERNAL_API_SECRET` | — | Secret for internal API callbacks |

## Redis (RunPod)

| Variable | Default | Description |
|---|---|---|
| `REDIS_PASSWORD` | — | Redis AUTH password (required on RunPod) |

## Storage

| Variable | Default | Description |
|---|---|---|
| `STORAGE_BACKEND` | `minio` | Storage backend |
| `MINIO_ACCESS_KEY` | `vexa-access-key` | MinIO access key |
| `MINIO_SECRET_KEY` | `vexa-secret-key` | MinIO secret key |
| `MINIO_BUCKET` | `vexa-recordings` | MinIO bucket name |

## Vector DB

| Variable | Default | Description |
|---|---|---|
| `QDRANT_API_KEY` | — | Qdrant API key (optional) |

## AI Integrations (Optional)

| Variable | Description |
|---|---|
| `OPENAI_API_KEY` | OpenAI API key (TTS, agent) |
| `OPENAI_BASE_URL` | OpenAI base URL (default: `https://api.openai.com`) |
| `ANTHROPIC_API_KEY` | Anthropic API key (agent) |
| `VEXA_TRANSCRIBER_API_KEY` | Transcription service API key |
| `ZOOM_CLIENT_ID` | Zoom OAuth client ID |
| `ZOOM_CLIENT_SECRET` | Zoom OAuth client secret |
| `TTS_API_TOKEN` | TTS service auth token |

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
| `RUNPOD_API_KEY` | — | Local/dev fallback for the RunPod account API key |
| `RUNPOD_ACCOUNT_API_KEY` | — | Preferred RunPod account API key inside deployed stateful pods |
| `RUNPOD_GPU_TYPE` | `NVIDIA GeForce RTX 3090` | GPU type for bot pods |
| `RUNPOD_GPU_TYPES` | `NVIDIA GeForce RTX 3090,NVIDIA GeForce RTX 5090,NVIDIA RTX A5000,NVIDIA RTX A4000` | Ordered GPU fallback list for bot pods |
| `RUNPOD_CLOUD_TYPE` | `COMMUNITY` | Cloud tier (`SECURE` or `COMMUNITY`) |
| `RUNPOD_CONTAINER_DISK_GB` | `40` | Container disk size for bot pods |
| `RUNPOD_POLL_INTERVAL` | `15` | Pod status poll interval (seconds) |
| `BOT_IMAGE` | `kyomoto/kioku-stateless:latest` | Image for bot pods |
| `BROWSER_IMAGE` | defaults to `BOT_IMAGE` | Browser/runtime image passed into the meeting profile |
| `BOT_REDIS_URL` | defaults to `REDIS_URL` | Redis URL passed to bot pods |
| `BOT_MEETING_API_URL` | defaults to `MEETING_API_URL` | Meeting API URL passed to bot pods |

## Build Paths

| Variable | Default | Description |
|---|---|---|
| `KIOKU_VEXA_PATH` | `../../services/vexa` | Path to the checked-out Vexa submodule (for Docker builds) |
| `HIVEMIND_PATH` | `../../services/hivemind` | Path to hivemind source (for Docker builds) |
| `CONTAINER_DISK_GB` | `20` | Default container disk used by `deployment/runpod/deploy.sh` for the stateful CPU pod |
| `STATEFUL_RUNPOD_CLOUD_TYPE` | `COMMUNITY` | Cloud tier used by `deployment/runpod/deploy.sh` for the long-lived stateful pod |
