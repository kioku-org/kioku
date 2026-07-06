"""Meeting API configuration — environment variables and defaults."""

import os

# Required
REDIS_URL = os.environ.get("REDIS_URL")
if not REDIS_URL:
    raise ValueError("Missing required environment variable: REDIS_URL")

# Runtime API — where we delegate container operations.
# Local/RunPod backend selection used to be a separate `router` service
# (services/router, proxying based on USE_LOCAL_RESOURCE/LOCAL_BOT_THRESHOLD);
# that's folded directly into meeting-api now (see choose_runtime_backend in
# meetings.py) rather than resurrecting a standalone proxy.
USE_LOCAL_RESOURCE = os.environ.get("USE_LOCAL_RESOURCE", "true").lower() == "true"
try:
    LOCAL_BOT_THRESHOLD = int(os.environ.get("LOCAL_BOT_THRESHOLD", "3"))
except ValueError:
    LOCAL_BOT_THRESHOLD = 3
LOCAL_BACKEND_URL = os.environ.get(
    "LOCAL_BACKEND_URL", os.environ.get("RUNTIME_API_URL", "http://localhost:8091")
)
RUNPOD_BACKEND_URL = os.environ.get("RUNPOD_BACKEND_URL", "http://localhost:8092")
RUNTIME_API_TOKEN = os.environ.get("RUNTIME_API_TOKEN", "")

# Self URL — used for callback_url in container creation
MEETING_API_URL = os.environ.get("MEETING_API_URL", "http://meeting-api:8080")

# Bot-facing URLs — passed to bot pods via BOT_CONFIG.
# On RunPod, bots need the stateful pod's public IP, while internal services
# use localhost. Defaults to the internal URLs for Docker Compose compatibility.
BOT_REDIS_URL = os.environ.get("BOT_REDIS_URL", REDIS_URL)
BOT_MEETING_API_URL = os.environ.get("BOT_MEETING_API_URL", MEETING_API_URL)
# Bot image / profile
BOT_IMAGE_NAME = os.environ.get("BOT_IMAGE_NAME", "vexaai/vexa-bot:latest")

# CORS
_cors_raw = os.getenv("CORS_ORIGINS", "*").strip()
CORS_WILDCARD = _cors_raw == "*"
CORS_ORIGINS = ["*"] if CORS_WILDCARD else [
    origin.strip()
    for origin in _cors_raw.split(",")
    if origin.strip()
]

# Delayed stop timeout for fallback container shutdown
try:
    BOT_STOP_DELAY_SECONDS = max(0, int(os.getenv("BOT_STOP_DELAY_SECONDS", "90")))
except ValueError:
    BOT_STOP_DELAY_SECONDS = 90

# Transcription collector
TRANSCRIPTION_COLLECTOR_URL = os.getenv(
    "TRANSCRIPTION_COLLECTOR_URL",
    "http://transcription-collector:8000",
)

# Hivemind — vector store / MCP service
HIVEMIND_URL = os.getenv("HIVEMIND_URL", "http://localhost:9100")

# Post-meeting hooks (comma-separated URLs)
POST_MEETING_HOOKS = [
    url.strip()
    for url in os.getenv("POST_MEETING_HOOKS", "").split(",")
    if url.strip()
]

# Recording metadata mode
def get_recording_metadata_mode() -> str:
    return os.getenv("RECORDING_METADATA_MODE", "meeting_data").strip().lower()
