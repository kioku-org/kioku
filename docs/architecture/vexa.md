---
title: "Vexa"
---
Vexa is the meeting-bot platform vendored into Kioku from [Vexa-ai/vexa](https://github.com/Vexa-ai/vexa).

## Services

| Service | Port | Purpose |
|---|---|---|
| api-gateway | 8000 | Public API entry point |
| admin-api | 8001 | Admin operations |
| meeting-api | 8080 | Bot lifecycle, meeting records |
| agent-api | 8100 | AI agent integration |
| runtime-api | 8090 | Container orchestration (Docker/K8s/Process/RunPod) |
| transcription-service | 80 | Whisper speech-to-text |
| tts-service | 8002 | Text-to-speech |
| mcp | 18888 | Vexa MCP server |
| redis | 6379 | Transcription streams, scheduling |
| minio | 9000 | Recording storage |
| vexa-bot | — | Playwright browser bot (joins meetings) |

## Runtime API Backends

The runtime-api orchestrates bot containers with pluggable backends:

| Backend | Env | Description |
|---|---|---|
| `docker` | `ORCHESTRATOR_BACKEND=docker` | Default. Uses Docker socket to spawn bot containers. |
| `kubernetes` | `ORCHESTRATOR_BACKEND=kubernetes` | K8s pods. |
| `process` | `ORCHESTRATOR_BACKEND=process` | Subprocesses (single-host, no Docker). |
| `runpod` | `ORCHESTRATOR_BACKEND=runpod` | RunPod REST API. Spawns GPU bot pods per meeting. |

## Bot Lifecycle

```
1. SPAWN
   POST /vexa/bots → Hivemind → Vexa meeting-api → runtime-api → bot container starts

2. MEETING
   Bot joins Google Meet/Zoom/Teams
   Audio captured → Whisper transcribes → Redis streams → meeting-api
   Bot sends status callbacks (joining, active, exited)

3. STOP (one of three paths)
   a) User stop: DELETE /vexa/bots → runtime-api stops container
   b) Bot self-exit: leaves meeting (everyone left / timeout)
   c) Scheduler timeout: 2h max → auto-stop

4. CLEANUP
   runtime-api detects exit → fires callback → meeting-api finalizes
   Transcript sent to Hivemind → embedded → searchable
```

## Custom: RunPod Backend

Kioku adds a custom `runpod` backend that spawns ephemeral GPU pods via the RunPod REST API:

- Bot pods run `kyomoto/kioku-stateless` image (Playwright + Whisper + GPU)
- `BOT_REDIS_URL` and `BOT_MEETING_API_URL` env vars point bots back to the stateful pod's public IP
- A Redis-backed reaper loop polls pod status every 15s for exit detection
- Pod is deleted on exit