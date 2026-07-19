#!/usr/bin/env bash
set -euo pipefail

# Shared-transcriber mode (whisper pool shard, see meeting_api/transcriber_pool.py):
# run only the transcription service, no bot. Env-based because docker/runpod
# backends pass Cmd as *arguments* to this entrypoint, not as a replacement.
# In-pod default is local whisper; cwd /app so kiku's model cache lands in
# /app/models (the BOT_MODEL_CACHE_DIR mount runtime-api adds to every pod).
export STT_BACKEND="${STT_BACKEND:-whisper}"

if [ "${WHISPER_ONLY:-}" = "true" ]; then
  echo "[KIOKU-WHISPER] Shared transcriber mode"
  cd /app
  exec /opt/transcription/kioku-transcription
fi

echo "[KIOKU-BOT] Starting bot pod..."

# Start the in-pod Whisper service unless BOT_CONFIG points transcription at a
# remote (shared) instance — then skip it and save ~2.1GB VRAM per bot.
# Pool-slot bots have no BOT_CONFIG yet at container start, so they keep the
# local service as a safe fallback (empty/localhost URL → local).
TX_URL=$(printf '%s' "${BOT_CONFIG:-}" | python3 -c 'import json,sys
try: print(json.load(sys.stdin).get("transcriptionServiceUrl") or "")
except Exception: print("")' 2>/dev/null || echo "")

TRANS_PID=""
case "$TX_URL" in
  ""|http://localhost*|http://127.0.0.1*)
    echo "[KIOKU-BOT] Starting transcription service..."
    cd /app
    /opt/transcription/kioku-transcription &
    TRANS_PID=$!
    # Give transcription service time to start
    sleep 3
    ;;
  *)
    echo "[KIOKU-BOT] Using shared transcription service at $TX_URL — skipping local whisper."
    ;;
esac

# Start the vexa bot (foreground — blocks until bot exits)
echo "[KIOKU-BOT] Starting vexa bot..."
cd /app/vexa-bot/core
/app/vexa-bot/entrypoint.sh &
BOT_PID=$!

# Wait for bot to exit
wait $BOT_PID
EXIT_CODE=$?

echo "[KIOKU-BOT] Bot exited with code $EXIT_CODE, cleaning up..."

# Kill transcription service (if we started one)
if [ -n "$TRANS_PID" ]; then
  kill $TRANS_PID 2>/dev/null || true
  wait $TRANS_PID 2>/dev/null || true
fi

echo "[KIOKU-BOT] Done."
exit $EXIT_CODE
