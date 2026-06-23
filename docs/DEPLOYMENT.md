# Deployment Guide

## Quick Start (Docker Compose)

### Prerequisites

- [Docker](https://docs.docker.com/get-docker/) with Compose v2
- NVIDIA GPU + [nvidia-container-toolkit](https://docs.nvidia.com/datacenter/cloud-native/container-toolkit/) (for Ollama embeddings)
- (Optional) Cloudflare Tunnel for public access

### Steps

```bash
cd deployment/docker

# 1. Bootstrap .env (copies template, generates secure secrets, pulls images)
./scripts/setup.sh

# 2. Fill in your API keys and domain
$EDITOR .env

# 3. (Optional) Configure Cloudflare Tunnel
cp cloudflared.yml.example cloudflared.yml
# Edit cloudflared.yml with your tunnel ID + domains
# Set CLOUDFLARED_CREDENTIALS_DIR in .env to your credentials folder

# 4. Start everything (stateful first, then stateless)
./scripts/manage.sh start

# 5. Verify all services are healthy
./scripts/healthcheck.sh
```

### Services

| Service | Port | Description |
|---|---|---|
| Hivemind API | `9100` | Core API (auth, sessions, knowledge search, MCP) |
| Vexa API Gateway | `8056` | Meeting bot API |
| Vexa Admin API | `8057` | Admin operations |
| Vexa MCP | `18888` | Vexa MCP server |
| MinIO Console | `9001` | Object storage UI |
| Ollama | `11434` | Local embedding model server |
| Qdrant | `6333` | Vector DB REST API |

### Management

```bash
./scripts/manage.sh status          # running containers + resource usage
./scripts/manage.sh logs <service>  # tail logs (e.g. logs kioku-hivemind)
./scripts/manage.sh stop            # stop all (data preserved)
./scripts/manage.sh down            # stop and remove containers
./scripts/manage.sh down-volumes    # destroy ALL data
./scripts/manage.sh backup          # dump databases to backups/
./scripts/manage.sh restore <file>  # restore from backup
```

---

## RunPod Deployment

Two-pod architecture:

| Pod | Type | Image | Lifecycle |
|---|---|---|---|
| Stateful | CPU | `kyomoto/kioku-stateful` | Always-on |
| Stateless | GPU | `kyomoto/kioku-stateless` | Ephemeral (per meeting) |

### Stateful Pod

Runs all always-on services: Postgres, Qdrant, Redis, MinIO, Ollama (CPU), Hivemind, all Vexa services, Cloudflared.

```bash
cd deployment/runpod
cp .env.example .env
# Fill in RUNPOD_API_KEY, secrets, domain
./deploy.sh
```

Exposed ports: `22` (SSH), `6379` (Redis+AUTH), `8080` (Meeting API), `9100` (Hivemind), `8056` (Vexa Gateway).

### Stateless Pod (Bot)

Spawned automatically by runtime-api when a meeting is requested. Runs:
- Vexa bot (Playwright + Chromium + Xvfb)
- Whisper transcription (GPU)

The pod exits when the bot exits. Cost: ~$0.27-0.46/hr (GPU pod, per meeting duration).

### CI/CD

GitHub Actions builds and pushes both images to Docker Hub on push to master:
- `kyomoto/kioku-stateful:latest`
- `kyomoto/kioku-stateless:latest`

The RunPod Integration Test workflow deploys a stateful pod, runs health checks, and cleans up.

---

## Environment Variables

See `deployment/docker/.env.example` (Docker) or `deployment/runpod/.env.example` (RunPod) for the full list.

### Required

| Variable | Description |
|---|---|
| `HIVEMIND_JWT_SECRET` | JWT signing secret (64-char hex) |
| `HIVEMIND_ENCRYPTION_SECRET` | API key encryption secret (64-char hex) |
| `VEXA_ADMIN_API_TOKEN` | Vexa admin API token |
| `DB_PASSWORD` | Postgres password |
| `REDIS_PASSWORD` | Redis AUTH password (RunPod only) |

### Optional

| Variable | Description |
|---|---|
| `OPENAI_API_KEY` | OpenAI API key (for TTS/agent) |
| `ANTHROPIC_API_KEY` | Anthropic API key (for agent) |
| `ZOOM_CLIENT_ID` / `ZOOM_CLIENT_SECRET` | Zoom OAuth (bot joining Zoom meetings) |
| `VEXA_TRANSCRIBER_API_KEY` | Transcription service API key |
| `CLOUDFLARED_CREDENTIALS_DIR` | Cloudflare tunnel credentials path |