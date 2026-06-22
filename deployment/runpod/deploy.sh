#!/usr/bin/env bash
# Deploy the kioku-stateful pod on RunPod using runpodctl.
# Runs: PostgreSQL 16 + pgvector, Qdrant v1.10.1 — persistent data layer.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

[[ -f "$SCRIPT_DIR/.env" ]] || {
    echo "Error: .env not found. Copy .env.example to .env and fill in your values."
    exit 1
}

# shellcheck source=/dev/null
source "$SCRIPT_DIR/.env"

[[ -n "${RUNPOD_API_KEY:-}" ]] || { echo "Error: RUNPOD_API_KEY not set in .env"; exit 1; }

IMAGE="${IMAGE:-ghcr.io/kioku-org/kioku-stateful:latest}"
POD_NAME="${POD_NAME:-kioku-stateful}"
CONTAINER_DISK="${CONTAINER_DISK_GB:-30}"
VOLUME_SIZE="${VOLUME_GB:-50}"

echo "Deploying $POD_NAME..."
echo "Image:         $IMAGE"
echo "Container disk: ${CONTAINER_DISK}GB"
echo "Volume:         ${VOLUME_SIZE}GB → /data"
echo ""

runpodctl create pod \
    --name "$POD_NAME" \
    --imageName "$IMAGE" \
    --containerDiskSize "$CONTAINER_DISK" \
    --volumeSize "$VOLUME_SIZE" \
    --volumePath "/data" \
    --ports "22/tcp,5432/tcp,6334/http" \
    --env "DB_NAME=${DB_NAME:-kioku}" \
    --env "DB_USER=${DB_USER:-kioku}" \
    --env "DB_PASSWORD=${DB_PASSWORD:-kioku}" \
    --env "PUBLIC_KEY=${PUBLIC_KEY:-}"

echo ""
echo "Pod created."
echo "  Check status: runpodctl get pod"
echo "  Remove pod:   ./destroy.sh <pod-id>"
