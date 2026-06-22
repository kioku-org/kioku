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

USE_LOCAL=false
while [[ $# -gt 0 ]]; do
    case "$1" in
        --local) USE_LOCAL=true; shift ;;
        *) error "Unknown argument: $1" ;;
    esac
done

set -a
while IFS='=' read -r key value; do
    [[ "$key" =~ ^#.*$ || -z "$key" ]] && continue
    value="${value%\"}"
    value="${value#\"}"
    export "$key"="$value"
done < "$SCRIPT_DIR/.env"
set +a

[[ -n "${RUNPOD_API_KEY:-}" ]] || error "RUNPOD_API_KEY not set"

info "Creating stateful CPU Pod..."

if [[ "$USE_LOCAL" == true ]]; then
    info "Using local entrypoint-stateful.sh..."
    ENTRYPOINT_B64="$(base64 -w0 "$SCRIPT_DIR/entrypoint-stateful.sh")"
    START_CMD="[\"/bin/bash\", \"-c\", \"apt-get update > /dev/null 2>&1 && apt-get install -y curl wget gnupg lsb-release sudo tzdata zstd > /dev/null 2>&1 && echo '${ENTRYPOINT_B64}' | base64 -d > /tmp/init.sh && chmod +x /tmp/init.sh && /tmp/init.sh\"]"
else
    INIT_URL="https://raw.githubusercontent.com/kioku-org/kioku/feat/runpod/deployment/runpod/entrypoint-stateful.sh"
    START_CMD="[\"/bin/bash\", \"-c\", \"apt-get update > /dev/null 2>&1 && apt-get install -y curl wget > /dev/null 2>&1 && curl -sL ${INIT_URL} -o /tmp/init.sh && chmod +x /tmp/init.sh && /tmp/init.sh\"]"
fi

RESPONSE=$(curl -s -X POST "https://rest.runpod.io/v1/pods" \
    -H "Authorization: Bearer $RUNPOD_API_KEY" \
    -H "Content-Type: application/json" \
    -d @- <<PAYLOAD
{
    "name": "kioku-stateful",
    "imageName": "ubuntu:22.04",
    "cloudType": "SECURE",
    "computeType": "CPU",
    "cpuFlavorIds": ["cpu5m"],
    "containerDiskInGb": 30,
    "volumeInGb": 50,
    "volumeMountPath": "/data",
    "ports": ["5432/tcp", "6379/tcp", "6334/http", "9000/http", "9001/http", "11434/http", "22/tcp"],
    "dockerStartCmd": ${START_CMD}
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
