---
title: "Docker Compose"
---
Deploy Kioku locally or on a VPS with Docker Compose.

## Prerequisites

- [Docker](https://docs.docker.com/get-docker/) with Compose v2
- NVIDIA GPU + [nvidia-container-toolkit](https://docs.nvidia.com/datacenter/cloud-native/container-toolkit/) (for Ollama embeddings)
- (Optional) Cloudflare Tunnel for public access

## Setup

```bash
cd deployment/docker

# 1. Bootstrap .env (copies template, generates secure secrets, pulls images)
./scripts/setup.sh

# 2. Fill in your API keys and domain
$EDITOR .env

# 3. (Optional) Configure Cloudflare Tunnel
cp cloudflared.yml.example cloudflared.yml
# Edit cloudflared.yml with your tunnel ID + domains
# Set CLOUDFLARED_CREDENTIALS_DIR in .env

# 4. Start everything (stateful first, then stateless)
./scripts/manage.sh start

# 5. Verify health
./scripts/healthcheck.sh
```


## Architecture

| Compose File | Services |
|---|---|
| `docker-compose.stateful.yml` | Postgres (pgvector), Qdrant |
| `docker-compose.stateless.yml` | Hivemind, all Vexa services, Ollama, Redis, MinIO, Cloudflared |

Stateful services start first (data volumes), then stateless services connect to them.

## Services

| Service | Port | Description |
|---|---|---|
| Hivemind API | `9100` | Core API (auth, sessions, knowledge search, MCP) |
| Vexa API Gateway | `8056` | Meeting bot API |
| Vexa Admin API | `8057` | Admin operations |
| Vexa MCP | `18888` | Vexa MCP server |
| MinIO Console | `9001` | Object storage UI |
| Ollama | `11434` | Local embedding model server |
| Qdrant | `6333` | Vector DB REST API |

## Management Commands

```bash
./scripts/manage.sh start               # start all services
./scripts/manage.sh stop                # stop all (data preserved)
./scripts/manage.sh down               # stop and remove containers
./scripts/manage.sh down-volumes        # destroy ALL data
./scripts/manage.sh restart             # restart all services
./scripts/manage.sh status              # show running containers + resource usage
./scripts/manage.sh logs [service]      # tail logs
./scripts/manage.sh backup              # dump databases to backups/
./scripts/manage.sh restore <file>      # restore from backup
./scripts/manage.sh shell [service]     # open shell in a container
./scripts/manage.sh db-shell            # open psql shell
./scripts/manage.sh start-stateful      # start only postgres + qdrant
./scripts/manage.sh stop-stateful       # stop only postgres + qdrant
```

## Cloudflare Tunnel

For public access without opening inbound ports, use Cloudflare Tunnel:

1. Create a tunnel at [Cloudflare Zero Trust](https://one.dash.cloudflare.com/)
2. Download the credentials JSON to your machine
3. Set `CLOUDFLARED_CREDENTIALS_DIR` in `.env` to the folder containing the JSON
4. Copy and edit the tunnel config:

```bash
cp cloudflared.yml.example cloudflared.yml
```

5. Edit with your tunnel ID and domains:

```yaml
tunnel: <your-tunnel-id>
credentials-file: /etc/cloudflared/creds/<your-tunnel-id>.json

ingress:
  - hostname: api.example.com
    service: http://kioku-hivemind:9100
  - hostname: meetings.example.com
    service: http://vexa-api-gateway:8000
  - service: http_status:404
```
