---
title: "For Self-Hosting"
description: "Run the full Kioku stack on your own hardware or VPS."
---

## Requirements

- Docker with Compose v2
- NVIDIA GPU + [nvidia-container-toolkit](https://docs.nvidia.com/datacenter/cloud-native/container-toolkit/) (for Ollama embeddings and Whisper transcription)
- 16 GB RAM minimum; 32 GB recommended for 3+ simultaneous bots
- Linux host (Ubuntu 22.04 recommended)

## Quick Start

```bash
git clone https://github.com/kioku-org/kioku.git
cd kioku/deployment/docker

# Copy .env template and fill in secrets
cp .env.example .env
$EDITOR .env
```

### Required env vars

```bash
HIVEMIND_JWT_SECRET=<64-char hex>           # openssl rand -hex 32
HIVEMIND_ENCRYPTION_SECRET=<64-char hex>    # openssl rand -hex 32
VEXA_ADMIN_API_TOKEN=<random token>
NEXTAUTH_SECRET=<random secret>             # openssl rand -base64 32
NEXTAUTH_URL=https://dashboard.yourdomain.com
VEXA_PUBLIC_URL=https://meetings.yourdomain.com
DOCKER_GID=<docker group GID>              # getent group docker | cut -d: -f3
```

### Start the stack

```bash
docker compose -f docker-compose.stateful.yml up -d
```

### Verify

```bash
curl http://localhost:8056/           # {"message":"Welcome to the Vexa API Gateway"}
curl http://localhost:9100/health     # {"status":"ok"}
curl http://localhost:3001            # 200 (dashboard)
curl http://localhost:18888/health    # {"status":"ok"} (MCP)
```

## Create Your First Admin User

```bash
curl -X POST http://localhost:8056/admin/users \
  -H "X-Admin-Token: $VEXA_ADMIN_API_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"email": "you@example.com"}'
```

Then sign in at `http://localhost:3001`.

## Public Access with Cloudflare Tunnel

To expose Kioku publicly without opening inbound ports:

1. Create a tunnel at [Cloudflare Zero Trust](https://one.dash.cloudflare.com/)
2. Download the credentials JSON
3. Create `cloudflared.yml`:

```yaml
tunnel: <your-tunnel-id>
credentials-file: /etc/cloudflared/creds/<your-tunnel-id>.json

ingress:
  - hostname: api.yourdomain.com
    service: http://localhost:9100
  - hostname: meetings.yourdomain.com
    service: http://localhost:8056
  - hostname: dashboard.yourdomain.com
    service: http://localhost:3001
  - hostname: mcp.yourdomain.com
    service: http://localhost:18888
  - service: http_status:404
```

4. Set in `.env`:
```bash
CLOUDFLARED_CREDENTIALS_DIR=/path/to/cloudflared/creds
```

The `cloudflared` process is managed by supervisord inside the stateful container.

## Install the CLI

```bash
cd services/cli
cargo install --path crates/cc-cli

# Point at your server
export KIOKU_SERVER=http://localhost:9100

kioku register-admin    # first-time bootstrap
kioku signin
```

## Persistence

All data is stored in named Docker volumes:

| Volume | Contents |
|---|---|
| `kioku-postgres-data` | All relational data |
| `kioku-qdrant-data` | Vector embeddings |
| `kioku-minio-data` | Meeting recordings |
| `kioku-redis-data` | Transcription streams |
| `kioku-ollama-data` | Embedding model weights |
| `kioku-cookie-data` | Bot browser session cookies |

These survive container restarts and `docker compose down`. Only `docker volume rm` deletes them.

## Upgrading

```bash
docker compose -f docker-compose.stateful.yml pull
docker compose -f docker-compose.stateful.yml up -d
```

Images are tagged `latest` and rebuilt by CI on every push to master.
