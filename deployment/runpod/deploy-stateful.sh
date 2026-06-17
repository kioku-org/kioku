#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

info()  { echo -e "${GREEN}[INFO]${NC} $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC} $*"; }
error() { echo -e "${RED}[ERROR]${NC} $*"; exit 1; }

set -a
while IFS='=' read -r key value; do
    [[ "$key" =~ ^#.*$ || -z "$key" ]] && continue
    value="${value%\"}"
    value="${value#\"}"
    export "$key"="$value"
done < "$SCRIPT_DIR/.env"
set +a

[[ -n "${RUNPOD_API_KEY:-}" ]] || error "RUNPOD_API_KEY not set"

INIT_CMD='#!/bin/bash\nset -e\necho "[KIOKU] Installing stateful services..."\napt-get update\napt-get install -y curl wget gnupg2 sudo zstd postgresql postgresql-common postgresql-client redis-server supervisor > /dev/null 2>&1\necho "[KIOKU] Installing Qdrant..."\ncurl -sL https://github.com/qdrant/qdrant/releases/download/v1.10.1/qdrant-x86_64-unknown-linux-gnu.tar.gz | tar xz -C /usr/local/bin/\necho "[KIOKU] Installing Ollama..."\ncurl -fsSL https://ollama.com/install.sh | sh > /dev/null 2>&1\necho "[KIOKU] Installing MinIO..."\ncurl -sL https://dl.min.io/server/minio/release/linux-amd64/minio -o /usr/local/bin/minio && chmod +x /usr/local/bin/minio\necho "[KIOKU] Configuring PostgreSQL..."\nmkdir -p /var/run/postgresql && chown postgres:postgres /var/run/postgresql\nmkdir -p /data/qdrant /data/minio\nif [ ! -f /var/lib/postgresql/data/PG_VERSION ]; then\n  su - postgres -c "/usr/lib/postgresql/14/bin/initdb -D /var/lib/postgresql/data"\n  echo "host all all 0.0.0.0/0 trust" >> /var/lib/postgresql/data/pg_hba.conf\necho "listen_addresses='"'"'*'"'"'" >> /var/lib/postgresql/data/postgresql.conf\n  su - postgres -c "/usr/lib/postgresql/14/bin/pg_ctl -D /var/lib/postgresql/data start"\n  su - postgres -c "psql -c \"CREATE USER kioku WITH PASSWORD '"'"'kioku'"'"';\""\n  su - postgres -c "psql -c \"CREATE DATABASE kioku OWNER kioku;\""\n  su - postgres -c "psql -d kioku -c \"CREATE EXTENSION IF NOT EXISTS vector;\""\n  su - postgres -c "psql -d kioku -c \"CREATE EXTENSION IF NOT EXISTS \\\"uuid-ossp\\\";\""\n  su - postgres -c "psql -d kioku -c \"CREATE SCHEMA IF NOT EXISTS hivemind;\""\n  su - postgres -c "psql -d kioku -c \"CREATE SCHEMA IF NOT EXISTS vexa;\""\n  su - postgres -c "/usr/lib/postgresql/14/bin/pg_ctl -D /var/lib/postgresql/data stop"\nfi\necho "[KIOKU] Starting services..."\nsupervisord -c /dev/stdin << SUPERVISORD\n[supervisord]\nnodaemon=true\nlogfile=/var/log/supervisord.log\n\n[program:postgresql]\ncommand=/usr/lib/postgresql/14/bin/postgres -D /var/lib/postgresql/data\nuser=postgres\nautostart=true\nautorestart=true\n\n[program:redis]\ncommand=redis-server --appendonly yes\nautostart=true\nautorestart=true\n\n[program:qdrant]\ncommand=/usr/local/bin/qdrant --storage-path /data/qdrant\nautostart=true\nautorestart=true\n\n[program:ollama]\ncommand=/usr/local/bin/ollama serve\nenvironment=OLLAMA_HOST="0.0.0.0:11434"\nautostart=true\nautorestart=true\n\n[program:minio]\ncommand=/usr/local/bin/minio server /data/minio --console-address ":9001"\nautostart=true\nautorestart=true\nSUPERVISORD\necho "[KIOKU] All stateful services running!"'

info "Creating stateful CPU Pod..."
info "Flavor: cpu5c (5 vCPU)"

RESPONSE=$(curl -s -X POST "https://rest.runpod.io/v1/pods" \
    -H "Authorization: Bearer $RUNPOD_API_KEY" \
    -H "Content-Type: application/json" \
    -d @- <<PAYLOAD
{
    "name": "kioku-stateful",
    "imageName": "runpod/ubuntu:22.04",
    "cloudType": "SECURE",
    "computeType": "CPU",
    "cpuFlavorIds": ["cpu5c"],
    "containerDiskInGb": 30,
    "volumeInGb": 50,
    "volumeMountPath": "/data",
    "ports": ["5432/tcp", "6379/tcp", "6334/http", "9000/http", "9001/http", "11434/http", "22/tcp"],
    "dockerStartCmd": ["/bin/bash", "-c", "$(echo -e "$INIT_CMD")"]
}
PAYLOAD
)

if echo "$RESPONSE" | grep -q '"error"'; then
    error "API error: $RESPONSE"
fi

POD_ID=$(echo "$RESPONSE" | python3 -c "import sys,json; print(json.load(sys.stdin).get('id',''))" 2>/dev/null || echo "")

if [[ -z "$POD_ID" ]]; then
    error "Failed to create pod. Response: $RESPONSE"
fi

info "Pod created! ID: $POD_ID"
info "Waiting for IP..."

for i in $(seq 1 60); do
    POD_INFO=$(curl -s "https://rest.runpod.io/v1/pods/$POD_ID" \
        -H "Authorization: Bearer $RUNPOD_API_KEY")
    
    PUBLIC_IP=$(echo "$POD_INFO" | python3 -c "import sys,json; print(json.load(sys.stdin).get('publicIp',''))" 2>/dev/null || echo "")
    DESIRED_STATUS=$(echo "$POD_INFO" | python3 -c "import sys,json; print(json.load(sys.stdin).get('desiredStatus',''))" 2>/dev/null || echo "")
    
    if [[ -n "$PUBLIC_IP" && "$PUBLIC_IP" != "None" && "$PUBLIC_IP" != "" ]]; then
        info "Pod ready! IP: $PUBLIC_IP"
        break
    fi
    
    [[ "$DESIRED_STATUS" == "EXITED" || "$DESIRED_STATUS" == "TERMINATED" ]] && error "Pod exited: $DESIRED_STATUS"
    printf "\r  Waiting... (%ds)" $((i*5))
    sleep 5
done

echo ""
info "═══════════════════════════════════════════════════════════════"
info "  Kioku Stateful Pod Deployed!"
info "═══════════════════════════════════════════════════════════════"
info "  Pod ID:    $POD_ID"
info "  Public IP: ${PUBLIC_IP:-pending}"
info ""
info "  Services:"
info "    PostgreSQL: ${PUBLIC_IP:-?}:5432"
info "    Redis:      ${PUBLIC_IP:-?}:6379"
info "    Qdrant:     ${PUBLIC_IP:-?}:6334"
info "    Ollama:     ${PUBLIC_IP:-?}:11434"
info "    MinIO:      ${PUBLIC_IP:-?}:9001"
info "═══════════════════════════════════════════════════════════════"

cat > "$SCRIPT_DIR/.stateful-pod" <<EOF
POD_ID=$POD_ID
PUBLIC_IP=${PUBLIC_IP:-}
EOF

info "Pod info saved to .stateful-pod"
