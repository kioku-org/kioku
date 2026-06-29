---
title: "Docker Compose"
description: "Run Kioku on a single machine with Docker Compose."
---

## Architecture

Kioku uses a two-image architecture:

```mermaid
graph TD
    subgraph kioku-stateful["kioku-stateful container (always running)"]
        direction TB
        HM[Hivemind API :9100]
        MA[meeting-api :8080]
        RA[runtime-api :8090]
        TS[transcription-service :80]
        TTS[tts-service :8002]
        MCP[MCP server :18888]
        DB[(PostgreSQL :5432)]
        RD[(Redis :6379)]
        MN[(MinIO :9000)]
        QD[(Qdrant :6333)]
        OL[Ollama :11434]
        DA[Dashboard :3001]
        CF[cloudflared]
    end

    subgraph bots["kioku-stateless (per-meeting, ephemeral)"]
        direction TB
        BOT[Playwright bot]
        WH[faster-whisper :8000]
    end

    kioku-stateful -->|Docker socket spawn| bots
    bots -->|Redis stream transcription| RD
    bots -->|callback| MA
```

All stateful services run as processes inside a single supervisord-managed container. The bot image is spawned on-demand via the Docker socket and removed after the meeting ends.

## Files

```
deployment/docker/
├── docker-compose.stateful.yml   # stateful services
├── docker-compose.stateless.yml  # bot image reference (build only)
├── Dockerfile.stateful
├── Dockerfile.stateless
├── .env.example
└── entrypoint-stateful-runtime.sh
```

## Ports Exposed to Host

| Port | Service | URL |
|---|---|---|
| 9100 | Hivemind API | `localhost:9100` |
| 8056 | Vexa API gateway | `localhost:8056` |
| 3001 | Dashboard | `localhost:3001` |
| 18888 | MCP server | `localhost:18888` |

All other ports (PostgreSQL, Redis, Qdrant, etc.) stay internal to the container.

## Starting the Stack

```bash
cd deployment/docker
cp .env.example .env
# fill in secrets (see .env.example for required fields)

docker compose -f docker-compose.stateful.yml up -d
```

## Environment Variables

See `.env.example` for the full reference. Critical ones:

| Variable | Description |
|---|---|
| `HIVEMIND_JWT_SECRET` | JWT signing secret (64-char hex) |
| `HIVEMIND_ENCRYPTION_SECRET` | Field encryption key (64-char hex) |
| `VEXA_ADMIN_API_TOKEN` | Admin API token |
| `NEXTAUTH_SECRET` | Dashboard session secret |
| `NEXTAUTH_URL` | Dashboard public URL |
| `VEXA_PUBLIC_URL` | Meetings API public URL |
| `DOCKER_GID` | Docker group GID (for socket access) |
| `VEXA_BOT_IMAGE` | Bot image (`ghcr.io/kioku-org/kioku-stateless:latest`) |
| `USE_LOCAL_RESOURCE` | `true` to spawn bots via Docker socket |
| `LOCAL_BOT_THRESHOLD` | Max local bots before overflow to RunPod |
| `RUNPOD_API_KEY` | Required if `USE_LOCAL_RESOURCE=false` |

## Volumes

All data persists across restarts in named volumes:

| Volume | Contents |
|---|---|
| `kioku-postgres-data` | All relational data |
| `kioku-qdrant-data` | Vector embeddings |
| `kioku-minio-data` | Meeting recordings |
| `kioku-redis-data` | Transcription streams |
| `kioku-ollama-data` | Embedding model weights |
| `kioku-whisper-model` | Whisper model cache (shared with bot containers) |
| `kioku-cookie-data` | Bot session cookies |
| `kioku-tts-voices` | TTS voice models |

## Upgrading

```bash
docker compose -f docker-compose.stateful.yml pull
docker compose -f docker-compose.stateful.yml up -d
```

## Common Operations

```bash
# View all process status (inside container)
docker exec kioku-stateful supervisorctl status

# View logs for a specific service
docker exec kioku-stateful supervisorctl tail -f meeting-api

# Restart a service
docker exec kioku-stateful supervisorctl restart runtime-api-local

# Shell into the container
docker exec -it kioku-stateful bash
```
