---
title: "Vexa"
---
Vexa is the meeting-bot platform tracked in Kioku as a Git submodule from
[kioku-org/vexa](https://github.com/kioku-org/vexa), which carries Kioku's RunPod backend,
workspace-schema split, and voice/agent extensions on top of the upstream Vexa codebase.

## Services

| Service | Port | Framework | Purpose |
|---|---|---|---|
| api-gateway | **8056** | Rust (axum) | Public API entry point |
| admin-api | **8001** | Python (FastAPI) | User/token management, internal validation |
| meeting-api | **8080** | Python (FastAPI) | Bot lifecycle, transcription collector, recordings, webhooks |
| agent-api | **8100** | Python (FastAPI) | In-meeting AI agent (chat/workspace files) |
| runtime-api-local | 8091 | Python (FastAPI) | Container orchestration — Docker backend |
| runtime-api-runpod | 8092 | Python (FastAPI) | Container orchestration — RunPod backend (only runs if `RUNPOD_API_KEY` is set) |
| cookie | 8099 | Python (FastAPI) | Stores browser session cookies for authenticated bot mode |
| tts-service | 8002 | Python | Piper text-to-speech |
| transcription-service | 8000 | Python | Standalone faster-whisper server (also embedded per-pod, see below) |
| mcp | 18888 | Rust (`kioku-mcp`) | The one unified MCP server — see [MCP overview](/mcp/overview) |
| redis | 6379 | — | Transcription streams, bot pool/reaper coordination |
| minio | 9000 | — | Recording storage |
| vexa-bot | — | TypeScript (Playwright) | Browser bot that joins meetings |

<Note>
  There is no standalone `router` service. Earlier revisions had a separate process that picked between the local-Docker and RunPod backends; that logic is now folded directly into **meeting-api**'s `choose_runtime_backend()` (counts active local-backend meetings in Postgres and picks `LOCAL_BACKEND_URL` or `RUNPOD_BACKEND_URL`) and independently into **api-gateway**. If you see "router :8090" in older diagrams, treat it as a conceptual role these two services perform, not a real process.
</Note>

`api-gateway` is the real public entry point and is more than a reverse proxy: it validates
API keys against admin-api (with a 60s cache), enforces per-route-prefix scopes
(`bot`/`browser`/`tx`), forwards `/mcp` to the MCP service, and owns its own application
logic for VNC/CDP browser-remote-control proxying, transcript share links, a multiplexed
WebSocket (`/ws`), and live-meeting-context injection for the agent-chat routes.

## Runtime API backends

`runtime-api`'s `ORCHESTRATOR_BACKEND` env var selects between four pluggable backends:

| Backend | Env value | Description |
|---|---|---|
| `docker` | `ORCHESTRATOR_BACKEND=docker` | Default. Spawns bot containers via the Docker socket. |
| `kubernetes` | `ORCHESTRATOR_BACKEND=kubernetes` | K8s pods. |
| `process` | `ORCHESTRATOR_BACKEND=process` | Subprocesses (single-host, no Docker). |
| `runpod` | `ORCHESTRATOR_BACKEND=runpod` | RunPod REST API. Spawns ephemeral GPU pods per meeting. |

Container templates live in `profiles.yaml` (hot-reloaded via SIGHUP), with three profiles
today: `meeting` (the Playwright bot, GPU), `browser-session` (persistent authenticated
Chrome + VNC), and `agent` (a Claude Code CLI container for agent-api sessions).

## Bot Lifecycle

```
1. SPAWN
   kioku meet <link> (or POST /vexa/bots) → Hivemind → meeting-api → runtime-api → bot pod starts

2. MEETING
   Bot joins Google Meet/Zoom/Teams
   Audio captured → embedded faster-whisper transcribes → Redis stream → meeting-api collector
   Bot sends status callbacks (joining, awaiting_admission, started, status_change, exited)

3. STOP (one of three paths)
   a) User stop: kioku meet --kill <id> / DELETE /bots → runtime-api stops the pod
   b) Bot self-exit: alone-detection timeout or the human leaves
   c) Scheduler timeout: max meeting duration → auto-stop

4. CLEANUP
   runtime-api detects exit → fires callback → meeting-api finalizes the meeting
   Transcript sent to Hivemind → embedded → searchable
```

Supported platforms: **Google Meet, Zoom, Microsoft Teams**.

### Alone-detection

Google Meet and MS Teams both poll participant count and auto-leave after a configurable
timeout once everyone else has left (`everyoneLeftTimeout`, fed by meeting-api's
`BOT_MAX_TIME_LEFT_ALONE`, default 2 minutes) or if no one ever joins
(`startupAloneTimeoutSeconds`). **Zoom has no alone-detection at all** — a Zoom bot left
alone in an empty meeting will not self-leave on a timeout. This is a known open gap, not
yet filed as a GitHub issue.

### Beyond bot-in-meeting capture

meeting-api and agent-api also support features not covered elsewhere in these docs:
an in-tab browser-extension capture mode (`POST /extension/sessions`, no bot required),
and in-meeting voice/avatar/screen-share agent control (`/bots/{platform}/{id}/speak`,
`/chat`, `/screen`, `/avatar`, `/events`). These are real, shipped endpoints — ask in the
project if you need them documented in more depth.

## Custom: RunPod Backend

Kioku adds a custom `runpod` backend (`runtime_api/backends/runpod.py`) that spawns
ephemeral GPU pods via the RunPod REST API — this is 100% a Kioku addition, not present in
upstream Vexa.

- Bot pods run the `kioku-stateless` image (Playwright + embedded faster-whisper + GPU).
- **Key precedence**: `RUNPOD_ACCOUNT_API_KEY` is preferred over `RUNPOD_API_KEY` — RunPod
  auto-injects its own pod-scoped `RUNPOD_API_KEY` into every pod it hosts, which would
  otherwise shadow the account-level key needed to call RunPod's own API.
- **Hostname resolution**: when the stateful pod itself is RunPod-hosted
  (`RUNPOD_POD_ID` set), bot-facing URLs (`BOT_REDIS_URL`, `BOT_MEETING_API_URL`,
  `BOT_TTS_URL`, `BOT_COOKIE_URL`) resolve via `RUNPOD_PUBLIC_IP`+`RUNPOD_TCP_PORT_<port>`
  (raw TCP, e.g. Redis) or `https://<pod-id>-<port>.proxy.runpod.net` (HTTP services).
  Non-RunPod deploys fall back to the `kioku-stateful` container-name hostname.
- **Warm pool** (`MIN_BOT_POOL`, default `0`): when set above zero, a `_pool_loop` keeps
  that many idle bot pods pre-spawned (image already pulled, waiting on a Redis `BLPOP` for
  their real config) so a real meeting request can **claim** an idle pod instantly instead
  of cold-spawning and waiting on the image pull. `create()` tries `_claim_pool_slot()`
  first, falling back to a normal cold-spawn if the pool is empty.
- **Orphan reconciliation**: a `_reconcile_orphans()` pass runs every reaper tick, listing
  **every** pod on the RunPod account (not just Redis-tracked ones) and deleting any
  already-EXITED/TERMINATED pod matching the bot name prefixes (`meeting-`,
  `browser-session-`, `agent-`, `pool-`) — this catches pods that fell out of the Redis
  tracking registry, e.g. because the stateful pod (which also hosts Redis) itself got
  recreated mid-meeting.
- A reaper loop polls pod status every `RUNPOD_POLL_INTERVAL` seconds (default 15s) for
  exit detection; exited pods are deleted.

See [RunPod deployment](/deployment/runpod) for setup and [Environment Variables](/deployment/environment-variables) for the full RunPod-related config.
