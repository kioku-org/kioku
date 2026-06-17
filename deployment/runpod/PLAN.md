# Kioku Platform — RunPod Deployment Plan

> **Goal:** Deploy the full Kioku platform on RunPod with optimal price-to-performance.
> **Approach:** Split architecture — persistent CPU Pod for stateful services + Serverless GPU for compute.
> **Budget:** $3.92 remaining

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                    STATEFUL CPU POD                              │
│                 (Always On · ~$0.05/hr)                          │
│                                                                  │
│  ┌──────────┐  ┌───────┐  ┌────────┐  ┌────────┐  ┌─────────┐  │
│  │PostgreSQL │  │ Redis │  │ Qdrant │  │ Ollama │  │  MinIO  │  │
│  │  :5432    │  │ :6379 │  │ :6334  │  │:11434  │  │ :9001   │  │
│  └──────────┘  └───────┘  └────────┘  └────────┘  └─────────┘  │
│                                                                  │
│  Network Volume: /data (persistent across restarts)              │
│  Image: ubuntu:22.04 + native installs                          │
│  Config: 4 vCPU, 16GB RAM, 50GB disk                           │
└────────────────────────┬────────────────────────────────────────┘
                         │
                    internal network
                         │
┌────────────────────────▼────────────────────────────────────────┐
│               SERVERLESS GPU WORKERS                             │
│           (Scale to Zero · $0.46/hr when active)                │
│                                                                  │
│  ┌───────────────┐  ┌──────────────┐  ┌─────────────────────┐   │
│  │ kioku-hivemind│  │ vexa APIs    │  │ vexa-transcription  │   │
│  │     :9100     │  │ :8000-8100   │  │      :80            │   │
│  │  (Rust)       │  │  (Python)    │  │  (Whisper GPU)      │   │
│  └───────────────┘  └──────────────┘  └─────────────────────┘   │
│                                                                  │
│  Image: ghcr.io/kioku-org/kioku-worker:latest                   │
│  GPU: NVIDIA RTX 3090 (24GB)                                    │
│  Min replicas: 0 (scale to zero)                                │
│  Max replicas: 2                                                │
└─────────────────────────────────────────────────────────────────┘
```

---

## Cost Analysis

### Monthly Estimates (24/7 operation)

| Component | Type | Cost/hr | Cost/month |
|-----------|------|---------|------------|
| Stateful Pod (Postgres, Redis, Qdrant, Ollama, MinIO) | CPU Pod | ~$0.05 | ~$36 |
| Compute (Hivemind, Vexa APIs) | Serverless GPU (idle) | $0.00 | $0 |
| Compute (Hivemind, Vexa APIs) | Serverless GPU (active) | $0.46 | varies |
| Storage (50GB network volume) | Persistent | - | ~$3.50 |
| **Total (idle)** | | | **~$40/month** |
| **Total (4hr/day active)** | | | **~$80/month** |

### vs. Single GPU Pod (current approach)

| Approach | Idle cost | Active cost | Monthly (24/7) |
|----------|-----------|-------------|-----------------|
| Single GPU Pod | $0.46/hr | $0.46/hr | ~$331/month |
| **Split architecture** | **$0.05/hr** | $0.51/hr | **~$40-80/month** |
| **Savings** | | | **~$250-290/month** |

### With $3.92 Balance

| Approach | Estimated Runtime |
|----------|-------------------|
| Single GPU Pod | ~8.5 hours |
| **Split (CPU + Serverless)** | **~78 hours CPU** + per-second GPU |

---

## Phase 1: Stateful CPU Pod

### 1.1 Services to Run

| Service | Port | Purpose | Resource Need |
|---------|------|---------|---------------|
| PostgreSQL 14 + pgvector | 5432 | Primary database | 2GB RAM |
| Redis 7 | 6379 | Cache, streams | 512MB RAM |
| Qdrant v1.10.1 | 6334 | Vector database | 2GB RAM |
| Ollama | 11434 | Embedding server | 4GB RAM + GPU passthrough |
| MinIO | 9000/9001 | Object storage (recordings) | 1GB RAM |

**Total RAM estimate:** ~10GB (16GB pod recommended)

### 1.2 Docker Image

```dockerfile
FROM ubuntu:22.04

# System deps
RUN apt-get update && apt-get install -y \
    curl wget gnupg2 sudo zstd \
    postgresql postgresql-common postgresql-client \
    redis-server \
    supervisor \
    && rm -rf /var/lib/apt/lists/*

# Qdrant
RUN curl -sL https://github.com/qdrant/qdrant/releases/download/v1.10.1/qdrant-x86_64-unknown-linux-gnu.tar.gz \
    | tar xz -C /usr/local/bin/

# Ollama
RUN curl -fsSL https://ollama.com/install.sh | sh

# MinIO
RUN curl -sL https://dl.min.io/server/minio/release/linux-amd64/minio -o /usr/local/bin/minio && \
    chmod +x /usr/local/bin/minio
RUN curl -sL https://dl.min.io/client/mc/release/linux-amd64/mc -o /usr/local/bin/mc && \
    chmod +x /usr/local/bin/mc

# PostgreSQL setup
RUN mkdir -p /var/run/postgresql && chown postgres:postgres /var/run/postgresql

# Entrypoint
COPY deployment/runpod/entrypoint-stateful.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

EXPOSE 5432 6379 6334 9000 9001 11434

ENTRYPOINT ["/entrypoint.sh"]
```

### 1.3 Entrypoint Script

```bash
#!/bin/bash
set -e

# Initialize PostgreSQL
if [ ! -f /var/lib/postgresql/data/PG_VERSION ]; then
    su - postgres -c "initdb -D /var/lib/postgresql/data"
    # Configure pg_hba.conf for trust auth
    echo "host all all 0.0.0.0/0 trust" >> /var/lib/postgresql/data/pg_hba.conf
    echo "listen_addresses='*'" >> /var/lib/postgresql/data/postgresql.conf
    su - postgres -c "pg_ctl -D /var/lib/postgresql/data start"
    
    # Create database and schemas
    su - postgres -c "psql -c \"CREATE USER kioku WITH PASSWORD 'kioku';\""
    su - postgres -c "psql -c \"CREATE DATABASE kioku OWNER kioku;\""
    su - postgres -c "psql -d kioku -c \"CREATE EXTENSION IF NOT EXISTS vector;\""
    su - postgres -c "psql -d kioku -c \"CREATE EXTENSION IF NOT EXISTS \\\"uuid-ossp\\\";\""
    su - postgres -c "psql -d kioku -c \"CREATE SCHEMA IF NOT EXISTS hivemind;\""
    su - postgres -c "psql -d kioku -c \"CREATE SCHEMA IF NOT EXISTS vexa;\""
    
    su - postgres -c "pg_ctl -D /var/lib/postgresql/data stop"
fi

# Start services
exec supervisord -c /etc/supervisor/conf.d/supervisord.conf
```

### 1.4 Supervisord Config

```ini
[supervisord]
nodaemon=true
logfile=/var/log/supervisord.log

[program:postgresql]
command=/usr/lib/postgresql/14/bin/postgres -D /var/lib/postgresql/data
user=postgres
autostart=true
autorestart=true

[program:redis]
command=redis-server --appendonly yes
autostart=true
autorestart=true

[program:qdrant]
command=/usr/local/bin/qdrant --storage-path /data/qdrant
autostart=true
autorestart=true

[program:ollama]
command=/usr/local/bin/ollama serve
environment=OLLAMA_HOST="0.0.0.0:11434"
autostart=true
autorestart=true

[program:minio]
command=/usr/local/bin/minio server /data/minio --console-address ":9001"
autostart=true
autorestart=true
```

### 1.5 RunPod API Call

```json
{
    "name": "kioku-stateful",
    "imageName": "ghcr.io/kioku-org/kioku-stateful:latest",
    "cloudType": "SECURE",
    "computeType": "CPU",
    "cpuFlavorIds": ["cpu5c"],
    "containerDiskInGb": 30,
    "volumeInGb": 50,
    "volumeMountPath": "/data",
    "ports": ["5432/tcp", "6379/tcp", "6334/http", "9000/http", "9001/http", "11434/http", "22/tcp"],
    "env": {
        "DB_NAME": "kioku",
        "DB_USER": "kioku",
        "DB_PASSWORD": "kioku"
    }
}
```

**CPU Flavor Options:**
- `cpu3c` — 3 cores, cheapest
- `cpu5c` — 5 cores, recommended for this workload

---

## Phase 2: Serverless GPU Workers

### 2.1 Worker Image

```dockerfile
FROM runpod/pytorch:2.4.0-py3.11-cuda12.4.1-devel-ubuntu22.04

WORKDIR /app

# System deps
RUN apt-get update && apt-get install -y \
    curl git postgresql-client \
    && rm -rf /var/lib/apt/lists/*

# Python deps (consolidated requirements)
COPY services/voice/deploy/lite/requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt

# Copy all service code
COPY services/voice/ ./services/voice/
COPY services/hivemind/ ./services/hivemind/

# Build hivemind (Rust)
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"
RUN cd services/hivemind && cargo build --release

# Copy handler
COPY deployment/runpod/handler.py .

CMD ["python", "-u", "handler.py"]
```

### 2.2 Serverless Handler

The handler manages two modes:
1. **Hivemind API mode** — routes requests to the Rust backend
2. **Transcription mode** — runs Whisper on GPU for audio processing

```python
import runpod
import subprocess
import os

def handler(job):
    job_input = job.get("input", {})
    action = job_input.get("action", "health")
    
    if action == "health":
        return {"status": "healthy"}
    
    elif action == "hivemind":
        # Start hivemind and proxy request
        # Connect to stateful pod's Postgres/Redis/Qdrant
        pass
    
    elif action == "transcribe":
        # Run Whisper transcription on GPU
        pass
    
    return {"error": f"Unknown action: {action}"}

runpod.serverless.start(handler)
```

### 2.3 Serverless Endpoint Config

```json
{
    "name": "kioku-compute",
    "imageName": "ghcr.io/kioku-org/kioku-worker:latest",
    "gpuTypeIds": ["NVIDIA GeForce RTX 3090"],
    "gpuCount": 1,
    "minVCPUPerGPU": 8,
    "minRAMPerGPU": 32,
    "containerDiskInGb": 50,
    "volumeInGb": 100,
    "volumeMountPath": "/workspace",
    "minReplicas": 0,
    "maxReplicas": 2,
    "idleTimeout": 60,
    "env": {
        "STATEFUL_DB_HOST": "<CPU_POD_IP>",
        "STATEFUL_DB_PORT": "5432",
        "STATEFUL_DB_NAME": "kioku",
        "STATEFUL_DB_USER": "kioku",
        "STATEFUL_DB_PASSWORD": "kioku",
        "STATEFUL_REDIS_URL": "redis://<CPU_POD_IP>:6379",
        "STATEFUL_QDRANT_URL": "http://<CPU_POD_IP>:6334",
        "STATEFUL_OLLAMA_URL": "http://<CPU_POD_IP>:11434"
    }
}
```

---

## Phase 3: Networking & DNS

### 3.1 RunPod Networking

- **CPU Pod:** Internal IP only (not publicly exposed)
- **Serverless Endpoint:** Public URL provided by RunPod (e.g., `https://<endpoint-id>.runpod.io`)
- **Cloudflare Tunnel:** Routes `api.kioku.chat` and `meetings.kioku.chat` to the serverless endpoint

### 3.2 Hostinger DNS Setup

Go to **hPanel → Domains → kioku.chat → DNS / Nameservers → DNS Records**

Add these records:

| Type | Name | Points to | TTL |
|------|------|-----------|-----|
| **CNAME** | `api` | `<runpod-endpoint>.runpod.io` | 14400 |
| **CNAME** | `meetings` | `<runpod-endpoint>.runpod.io` | 14400 |

Or if using Cloudflare Tunnel:

| Type | Name | Points to | TTL |
|------|------|-----------|-----|
| **CNAME** | `api` | `<cloudflare-tunnel-id>.cfargotunnel.com` | Auto |
| **CNAME** | `meetings` | `<cloudflare-tunnel-id>.cfargotunnel.com` | Auto |

### 3.3 Cloudflare Tunnel (Optional)

If you already have a Cloudflare tunnel set up for `kioku.chat`:
1. Add new ingress rules for `api.kioku.chat` and `meetings.kioku.chat`
2. Point them to the RunPod serverless endpoint URL
3. The tunnel handles HTTPS termination

---

## Phase 4: Implementation Steps

### Step 1: Build & Push Stateful Image
```bash
cd deployment/runpod
docker build -t ghcr.io/kioku-org/kioku-stateful:latest -f Dockerfile.stateful .
docker push ghcr.io/kioku-org/kioku-stateful:latest
```

### Step 2: Deploy Stateful CPU Pod
```bash
# Via RunPod API or dashboard
# CPU Pod with 5 cores, 16GB RAM, 50GB volume
# Result: Internal IP for services to connect to
```

### Step 3: Build & Push Worker Image
```bash
docker build -t ghcr.io/kioku-org/kioku-worker:latest -f Dockerfile.worker .
docker push ghcr.io/kioku-org/kioku-worker:latest
```

### Step 4: Create Serverless Endpoint
```bash
# Via RunPod API or dashboard
# RTX 3090, scale to zero, connect to stateful pod's internal IP
# Result: Public endpoint URL
```

### Step 5: Configure DNS
```bash
# In Hostinger hPanel
# Add CNAME records for api.kioku.chat and meetings.kioku.chat
# Point to RunPod endpoint or Cloudflare tunnel
```

### Step 6: Test
```bash
# Health check
curl https://api.kioku.chat/health

# Auth test
curl -X POST https://api.kioku.chat/auth/register/admin \
  -H "Content-Type: application/json" \
  -d '{"company_name":"test","email":"test@kioku.chat","name":"Test","password":"test123"}'
```

---

## File Structure (New)

```
deployment/runpod/
├── .env                          # RunPod API key + config
├── .env.example                  # Template
├── .gitignore                    # Excludes .env, .pod-info
├── Dockerfile.stateful           # CPU Pod image (Postgres, Redis, Qdrant, Ollama, MinIO)
├── Dockerfile.worker             # Serverless GPU image (Hivemind, Vexa APIs, Transcription)
├── entrypoint-stateful.sh        # CPU Pod entrypoint
├── handler.py                    # Serverless handler
├── requirements.txt              # Consolidated Python deps
├── supervisord-stateful.conf     # CPU Pod process manager
├── deploy.sh                     # Deploy CPU Pod
├── deploy-worker.sh              # Deploy Serverless Endpoint
├── destroy.sh                    # Tear down
├── status.sh                     # Check status
└── PLAN.md                       # This file
```

---

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| CPU Pod crashes | Supervisord auto-restarts; network volume preserves data |
| Serverless cold start (~2-3min) | Acceptable for API calls; pre-warm with health checks |
| Ollama GPU in serverless | RunPod provides GPU passthrough automatically |
| Data loss on CPU Pod stop | Use network volume (persists across stops) |
| Budget exhaustion | Monitor RunPod dashboard; set up low balance alerts |

---

## Next Actions

1. ✅ Create PLAN.md (this file)
2. ⬜ Build stateful CPU Pod image
3. ⬜ Deploy CPU Pod and verify services
4. ⬜ Build serverless worker image
5. ⬜ Create serverless endpoint
6. ⬜ Configure DNS in Hostinger
7. ⬜ End-to-end testing
