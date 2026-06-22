#!/usr/bin/env bash
# Deploy the kioku-stateful pod on RunPod using runpodctl.
# Runs: all always-on services (postgres, qdrant, redis, minio, ollama,
# hivemind, vexa services, runtime-api, cloudflared).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

[[ -f "$SCRIPT_DIR/.env" ]] || {
    echo "Error: .env not found. Copy .env.example to .env and fill in your values."
    exit 1
}

# shellcheck source=/dev/null
source "$SCRIPT_DIR/.env"

[[ -n "${RUNPOD_API_KEY:-}" ]] || { echo "Error: RUNPOD_API_KEY not set in .env"; exit 1; }

IMAGE="${IMAGE:-kyomoto/kioku-stateful:latest}"
POD_NAME="${POD_NAME:-kioku-stateful}"
CONTAINER_DISK="${CONTAINER_DISK_GB:-40}"
VOLUME_SIZE="${VOLUME_GB:-50}"

echo "Deploying $POD_NAME..."
echo "Image:          $IMAGE"
echo "Container disk: ${CONTAINER_DISK}GB"
echo "Volume:         ${VOLUME_SIZE}GB → /data"
echo ""

ENV_FLAGS=(
    --env "DB_NAME=${DB_NAME:-kioku}"
    --env "DB_USER=${DB_USER:-kioku}"
    --env "DB_PASSWORD=${DB_PASSWORD:-kioku}"
    --env "REDIS_PASSWORD=${REDIS_PASSWORD:-kioku-redis}"
    --env "HIVEMIND_JWT_SECRET=${HIVEMIND_JWT_SECRET}"
    --env "HIVEMIND_ENCRYPTION_SECRET=${HIVEMIND_ENCRYPTION_SECRET}"
    --env "VEXA_ADMIN_API_TOKEN=${VEXA_ADMIN_API_TOKEN}"
    --env "INTERNAL_API_SECRET=${INTERNAL_API_SECRET:-}"
    --env "ORCHESTRATOR_BACKEND=runpod"
    --env "RUNPOD_API_KEY=${RUNPOD_API_KEY}"
    --env "BOT_IMAGE=${BOT_IMAGE:-kyomoto/kioku-stateless:latest}"
    --env "VEXA_PUBLIC_URL=${VEXA_PUBLIC_URL:-}"
    --env "CORS_ORIGINS=${CORS_ORIGINS:-*}"
    --env "LOG_LEVEL=${LOG_LEVEL:-INFO}"
    --env "VEXA_ENV=${VEXA_ENV:-production}"
    --env "STORAGE_BACKEND=${STORAGE_BACKEND:-minio}"
    --env "MINIO_ACCESS_KEY=${MINIO_ACCESS_KEY:-kioku}"
    --env "MINIO_SECRET_KEY=${MINIO_SECRET_KEY:-kioku-minio-password}"
    --env "MINIO_BUCKET=${MINIO_BUCKET:-vexa-recordings}"
    --env "RECORDING_ENABLED=${RECORDING_ENABLED:-false}"
    --env "QDRANT_API_KEY=${QDRANT_API_KEY:-}"
    --env "RUNPOD_GPU_TYPE=${RUNPOD_GPU_TYPE:-NVIDIA GeForce RTX 3090}"
    --env "RUNPOD_CLOUD_TYPE=${RUNPOD_CLOUD_TYPE:-COMMUNITY}"
)

# Optional API keys
[[ -n "${OPENAI_API_KEY:-}" ]] && ENV_FLAGS+=(--env "OPENAI_API_KEY=${OPENAI_API_KEY}")
[[ -n "${OPENAI_BASE_URL:-}" ]] && ENV_FLAGS+=(--env "OPENAI_BASE_URL=${OPENAI_BASE_URL}")
[[ -n "${ANTHROPIC_API_KEY:-}" ]] && ENV_FLAGS+=(--env "ANTHROPIC_API_KEY=${ANTHROPIC_API_KEY}")
[[ -n "${VEXA_TRANSCRIBER_API_KEY:-}" ]] && ENV_FLAGS+=(--env "VEXA_TRANSCRIBER_API_KEY=${VEXA_TRANSCRIBER_API_KEY}")
[[ -n "${ZOOM_CLIENT_ID:-}" ]] && ENV_FLAGS+=(--env "ZOOM_CLIENT_ID=${ZOOM_CLIENT_ID}")
[[ -n "${ZOOM_CLIENT_SECRET:-}" ]] && ENV_FLAGS+=(--env "ZOOM_CLIENT_SECRET=${ZOOM_CLIENT_SECRET}")
[[ -n "${TTS_API_TOKEN:-}" ]] && ENV_FLAGS+=(--env "TTS_API_TOKEN=${TTS_API_TOKEN}")
[[ -n "${PUBLIC_KEY:-}" ]] && ENV_FLAGS+=(--env "PUBLIC_KEY=${PUBLIC_KEY}")

runpodctl create pod \
    --name "$POD_NAME" \
    --imageName "$IMAGE" \
    --containerDiskSize "$CONTAINER_DISK" \
    --volumeSize "$VOLUME_SIZE" \
    --volumePath "/data" \
    --ports "22/tcp,6379/tcp,8080/http,9100/http,8056/http" \
    "${ENV_FLAGS[@]}"

echo ""
echo "Pod created."
echo "  Check status: runpodctl get pod"
echo "  Remove pod:   ./destroy.sh <pod-id>"
echo ""
echo "Bot pods (kyomoto/kioku-stateless) will be spawned automatically"
echo "by runtime-api when meetings are requested."
