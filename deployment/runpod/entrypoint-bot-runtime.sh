#!/usr/bin/env bash
set -euo pipefail

echo "[KIOKU-BOT] Starting bot pod..."

# Start Whisper transcription service in background
echo "[KIOKU-BOT] Starting transcription service..."
cd /opt/transcription
python3 main.py &
TRANS_PID=$!

# Give transcription service time to start
sleep 3

# Start the vexa bot (foreground — blocks until bot exits)
echo "[KIOKU-BOT] Starting vexa bot..."
cd /app/vexa-bot/core
/app/vexa-bot/entrypoint.sh &
BOT_PID=$!

# Wait for bot to exit
wait $BOT_PID
EXIT_CODE=$?

echo "[KIOKU-BOT] Bot exited with code $EXIT_CODE, cleaning up..."

# Kill transcription service
kill $TRANS_PID 2>/dev/null || true
wait $TRANS_PID 2>/dev/null || true

echo "[KIOKU-BOT] Done."
exit $EXIT_CODE
