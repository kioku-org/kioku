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

# ─── Load env ────────────────────────────────────────────────────────────────
[[ -f "$SCRIPT_DIR/.env" ]] || error ".env not found. Copy .env.example to .env"

set -a
while IFS='=' read -r key value; do
    [[ "$key" =~ ^#.*$ || -z "$key" ]] && continue
    value="${value%\"}"
    value="${value#\"}"
    export "$key"="$value"
done < "$SCRIPT_DIR/.env"
set +a

[[ -n "${RUNPOD_API_KEY:-}" ]] || error "RUNPOD_API_KEY not set"

# ─── Create Pod ──────────────────────────────────────────────────────────────
info "Creating RunPod Pod..."
info "GPU: ${RUNPOD_GPU_TYPE:-NVIDIA RTX 3090}"

RESPONSE=$(curl -s -X POST "https://rest.runpod.io/v1/pods" \
    -H "Authorization: Bearer $RUNPOD_API_KEY" \
    -H "Content-Type: application/json" \
    -d @- <<PAYLOAD
{
    "name": "${RUNPOD_POD_NAME:-kioku-platform}",
    "imageName": "runpod/pytorch:2.4.0-py3.11-cuda12.4.1-devel-ubuntu22.04",
    "cloudType": "SECURE",
    "gpuTypeIds": ["${RUNPOD_GPU_TYPE:-NVIDIA RTX 3090}"],
    "gpuCount": ${RUNPOD_GPU_COUNT:-1},
    "minVCPUPerGPU": ${RUNPOD_VCPU_PER_GPU:-8},
    "minRAMPerGPU": ${RUNPOD_RAM_PER_GPU:-32},
    "containerDiskInGb": ${RUNPOD_DISK_GB:-50},
    "ports": ["80/http", "443/http", "9100/http", "8056/http", "8057/http", "6334/http", "11434/http", "9001/http", "18888/http", "22/tcp"],
    "volumeInGb": ${RUNPOD_VOLUME_GB:-50},
    "volumeMountPath": "/workspace",
    "env": {
        "DB_NAME": "${DB_NAME:-kioku}",
        "DB_USER": "${DB_USER:-kioku}",
        "DB_PASSWORD": "${DB_PASSWORD:-kioku}"
    },
    "dockerStartCmd": ["/bin/bash", "-c", "cd /workspace && curl -sL https://raw.githubusercontent.com/kioku-app/kioku/feat/runpod/deployment/runpod/init-pod.sh | bash"]
}
PAYLOAD
)

# Check for errors
if echo "$RESPONSE" | grep -q '"code"'; then
    error "RunPod API error: $RESPONSE"
fi

POD_ID=$(echo "$RESPONSE" | python3 -c "import sys,json; print(json.load(sys.stdin).get('id',''))" 2>/dev/null || echo "")

if [[ -z "$POD_ID" ]]; then
    error "Failed to create pod. Response: $RESPONSE"
fi

info "Pod created successfully!"
info "Pod ID: $POD_ID"

# Wait for pod to get IP
info "Waiting for Pod to initialize..."
for i in $(seq 1 60); do
    POD_INFO=$(curl -s "https://rest.runpod.io/v1/pods/$POD_ID" \
        -H "Authorization: Bearer $RUNPOD_API_KEY")

    PUBLIC_IP=$(echo "$POD_INFO" | python3 -c "import sys,json; print(json.load(sys.stdin).get('publicIp',''))" 2>/dev/null || echo "")
    DESIRED_STATUS=$(echo "$POD_INFO" | python3 -c "import sys,json; print(json.load(sys.stdin).get('desiredStatus',''))" 2>/dev/null || echo "")
    PORT_MAPPINGS=$(echo "$POD_INFO" | python3 -c "import sys,json; pm=json.load(sys.stdin).get('portMappings',{}); print(json.dumps(pm))" 2>/dev/null || echo "{}")

    if [[ -n "$PUBLIC_IP" && "$PUBLIC_IP" != "None" ]]; then
        info "Pod is ready!"
        info "Public IP: $PUBLIC_IP"
        info "Port mappings: $PORT_MAPPINGS"
        break
    fi

    if [[ "$DESIRED_STATUS" == "EXITED" || "$DESIRED_STATUS" == "TERMINATED" ]]; then
        error "Pod exited unexpectedly: $DESIRED_STATUS"
    fi

    printf "\r  Waiting... (%ds)" $((i*5))
    sleep 5
done

echo ""
echo ""
info "═══════════════════════════════════════════════════════════════"
info "  Kioku Platform - RunPod Deployment"
info "═══════════════════════════════════════════════════════════════"
info ""
info "  Pod ID:     $POD_ID"
info "  Public IP:  ${PUBLIC_IP:-pending...}"
info "  Status:     $DESIRED_STATUS"
info ""
info "  Services (after init completes ~5-10 min):"
info "    API:        http://${PUBLIC_IP:-<IP>}:9100"
info "    Vexa:       http://${PUBLIC_IP:-<IP>}:8056"
info "    Admin:      http://${PUBLIC_IP:-<IP>}:8057"
info "    Qdrant:     http://${PUBLIC_IP:-<IP>}:6334"
info "    Ollama:     http://${PUBLIC_IP:-<IP>}:11434"
info "    MinIO:      http://${PUBLIC_IP:-<IP>}:9001"
info "    MCP:        http://${PUBLIC_IP:-<IP>}:18888"
info ""
info "  SSH: ssh root@${PUBLIC_IP:-<IP>} (port shown in RunPod dashboard)"
info ""
info "  Monitor: https://www.runpod.io/console/pods"
info "═══════════════════════════════════════════════════════════════"

# Save pod info
cat > "$SCRIPT_DIR/.pod-info" <<EOF
POD_ID=$POD_ID
PUBLIC_IP=${PUBLIC_IP:-}
POD_STATUS=$DESIRED_STATUS
PORT_MAPPINGS=$PORT_MAPPINGS
EOF

info "Pod info saved to .pod-info"
