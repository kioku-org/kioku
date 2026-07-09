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

set -a
# shellcheck source=/dev/null
source "$SCRIPT_DIR/.env"
set +a

require_env() {
    local name="$1"
    [[ -n "${!name:-}" ]] || {
        echo "Error: $name not set"
        exit 1
    }
}

require_env RUNPOD_API_KEY
require_env HIVEMIND_JWT_SECRET
require_env HIVEMIND_ENCRYPTION_SECRET
require_env VEXA_ADMIN_API_TOKEN

IMAGE="${IMAGE:-ghcr.io/kioku-org/kioku-stateful:latest}"
POD_NAME="${POD_NAME:-${RUNPOD_POD_NAME:-kioku-stateful}}"
CONTAINER_DISK="${CONTAINER_DISK_GB:-${RUNPOD_DISK_GB:-20}}"
VOLUME_SIZE="${VOLUME_GB:-${RUNPOD_VOLUME_GB:-50}}"
STATEFUL_RUNPOD_CLOUD_TYPE="${STATEFUL_RUNPOD_CLOUD_TYPE:-COMMUNITY}"
BOT_IMAGE="${BOT_IMAGE:-ghcr.io/kioku-org/kioku-stateless:latest}"
BROWSER_IMAGE="${BROWSER_IMAGE:-$BOT_IMAGE}"
RUNPOD_ACCOUNT_API_KEY="${RUNPOD_ACCOUNT_API_KEY:-$RUNPOD_API_KEY}"
RUNPOD_GPU_TYPES="${RUNPOD_GPU_TYPES:-NVIDIA GeForce RTX 3090,NVIDIA GeForce RTX 5090,NVIDIA RTX A5000,NVIDIA RTX A4000}"

export BOT_IMAGE
export BROWSER_IMAGE
export RUNPOD_ACCOUNT_API_KEY
export RUNPOD_GPU_TYPES
export STATEFUL_RUNPOD_CLOUD_TYPE

echo "Deploying $POD_NAME..."
echo "Image:          $IMAGE"
echo "Cloud:          $STATEFUL_RUNPOD_CLOUD_TYPE"
echo "Compute:        CPU"
echo "Container disk: ${CONTAINER_DISK}GB"
echo "Volume:         ${VOLUME_SIZE}GB → /data"
echo ""

ENV_JSON="$(python3 <<'PY'
import json
import os

keys = [
    "DB_NAME",
    "DB_USER",
    "DB_PASSWORD",
    "REDIS_PASSWORD",
    "HIVEMIND_JWT_SECRET",
    "HIVEMIND_ENCRYPTION_SECRET",
    "VEXA_ADMIN_API_TOKEN",
    "INTERNAL_API_SECRET",
    "ORCHESTRATOR_BACKEND",
    "RUNPOD_ACCOUNT_API_KEY",
    "BOT_IMAGE",
    "BROWSER_IMAGE",
    "VEXA_PUBLIC_URL",
    "CORS_ORIGINS",
    "LOG_LEVEL",
    "VEXA_ENV",
    "STORAGE_BACKEND",
    "MINIO_ACCESS_KEY",
    "MINIO_SECRET_KEY",
    "MINIO_BUCKET",
    "RECORDING_ENABLED",
    "QDRANT_API_KEY",
    "RUNPOD_GPU_TYPE",
    "RUNPOD_GPU_TYPES",
    "RUNPOD_CLOUD_TYPE",
    "OPENAI_API_KEY",
    "OPENAI_BASE_URL",
    "ANTHROPIC_API_KEY",
    "VEXA_TRANSCRIBER_API_KEY",
    "ZOOM_CLIENT_ID",
    "ZOOM_CLIENT_SECRET",
    "TTS_API_TOKEN",
    "PUBLIC_KEY",
    "NEXTAUTH_SECRET",
    "VEXA_ALLOW_DIRECT_LOGIN",
    "GOOGLE_CLIENT_ID",
    "GOOGLE_CLIENT_SECRET",
    "USE_LOCAL_RESOURCE",
    "LOCAL_BOT_THRESHOLD",
    "MIN_BOT_POOL",
]

data = {}
for key in keys:
    value = os.environ.get(key)
    if value:
        data[key] = value

data.setdefault("DB_NAME", "kioku")
data.setdefault("DB_USER", "kioku")
data.setdefault("DB_PASSWORD", "kioku")
data.setdefault("REDIS_PASSWORD", "kioku-redis")
data.setdefault("ORCHESTRATOR_BACKEND", "runpod")
data.setdefault("CORS_ORIGINS", "*")
data.setdefault("LOG_LEVEL", "INFO")
data.setdefault("VEXA_ENV", "production")
data.setdefault("STORAGE_BACKEND", "minio")
data.setdefault("MINIO_ACCESS_KEY", "kioku")
data.setdefault("MINIO_SECRET_KEY", "kioku-minio-password")
data.setdefault("MINIO_BUCKET", "vexa-recordings")
data.setdefault("RECORDING_ENABLED", "false")
data.setdefault("RUNPOD_GPU_TYPE", "NVIDIA GeForce RTX 3090")
data.setdefault(
    "RUNPOD_GPU_TYPES",
    "NVIDIA GeForce RTX 3090,NVIDIA GeForce RTX 5090,NVIDIA RTX A5000,NVIDIA RTX A4000",
)
data.setdefault("RUNPOD_CLOUD_TYPE", "COMMUNITY")
data.setdefault("VEXA_ALLOW_DIRECT_LOGIN", "true")
# This pod has no Docker socket to hand to runtime-api-local, so meeting-api's
# local/RunPod backend picker (choose_runtime_backend in meeting_api/meetings.py)
# must never choose "local" here — every bot spawn has to go through
# runtime-api-runpod instead. Default USE_LOCAL_RESOURCE off for this script;
# override only if you know this pod really does have a docker socket.
data.setdefault("USE_LOCAL_RESOURCE", "false")
data.setdefault("MIN_BOT_POOL", "0")
# Dashboard's NextAuth session cookie signing key — without it, login breaks.
# Not in .env.example's required list since most deploys inherit it from the
# dev-server .env; generate one so a from-scratch deploy still works.
if not data.get("NEXTAUTH_SECRET"):
    import secrets as _secrets

    data["NEXTAUTH_SECRET"] = _secrets.token_hex(32)

print(json.dumps(data, separators=(",", ":")))
PY
)"

CMD=(
    runpodctl pod create
    --name "$POD_NAME"
    --image "$IMAGE"
    --compute-type CPU
    --cloud-type "$STATEFUL_RUNPOD_CLOUD_TYPE"
    --container-disk-in-gb "$CONTAINER_DISK"
    --volume-in-gb "$VOLUME_SIZE"
    --volume-mount-path "/data"
    --ports "22/tcp,6379/tcp,8080/http,9100/http,8056/http,3001/http,18888/http,8002/http,8099/http"
    --env "$ENV_JSON"
)

if [[ "$STATEFUL_RUNPOD_CLOUD_TYPE" == "COMMUNITY" ]]; then
    CMD+=(--public-ip)
fi

"${CMD[@]}"

echo ""
echo "Pod created."
echo "  Check status: runpodctl get pod"
echo "  Remove pod:   ./destroy.sh <pod-id>"
echo ""
echo "Bot pods (${BOT_IMAGE}) will be spawned automatically"
echo "by runtime-api when meetings are requested."
