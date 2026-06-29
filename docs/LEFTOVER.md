# LEFTOVER

Last updated: 2026-06-30 (rev 8)

## Current Status

True stateful/stateless architecture deployed and running on the deploy server. All 18 supervisord
processes in `kioku-stateful` come up cleanly. Bot containers spawn on-demand via Docker socket.
`kioku-stateless` image builds from `deployment/docker/Dockerfile.stateless`. CI pushes both images
to GHCR on every push to master.

## Architecture

```
kioku-stateful (one always-on container, supervisord-managed)
  Infrastructure:   postgres, qdrant, redis, minio, ollama
  Vexa backends:    api-gateway (8056), admin-api (8001), meeting-api (8080),
                    agent-api (8100), tts (8002)
  Runtime:          runtime-api-local (8091), runtime-api-runpod (8092, only if RUNPOD_API_KEY set),
                    router (8090, proxies local↔runpod)
  Kioku-owned:      hivemind (9100), mcp (18888), dashboard (3001),
                    cookie (8099), cloudflared
  Process mgr:      supervisord

kioku-stateless (ephemeral per-meeting pod, spawned by runtime-api-local)
  Browser:          Playwright + Chromium + Xvfb + PulseAudio
  Bot:              vexa-bot (TypeScript)
  Transcription:    faster-whisper embedded (localhost:8000 inside pod)
  Model:            shared volume mount — one download, all bots reuse it
```

Bot containers run on `kioku-network` and reach stateful services by container name (`kioku-stateful`).

## What Is Done

### Refactor (issue #40 closed)
- Removed `services/vexa` submodule — all source now lives in `services/`
- Rewrote `Dockerfile.stateful` as a 4-stage multi-stage build:
  - Stage 1: Rust (hivemind binary)
  - Stage 2: python-builder (all Python services in one venv)
  - Stage 3: dashboard-builder (Next.js standalone)
  - Stage 4: CUDA runtime (nvidia/cuda:12.3.2-cudnn9-runtime-ubuntu22.04)
- `docker-compose.stateful.yml` reduced to single service
- `docker-compose.stateless.yml` repurposed as model-warmup helper
- CI updated: builds `kioku-stateful` + `kioku-stateless` from `deployment/docker/`; dropped dead
  `build-mcp`, `build-runtime-router`, `build-dashboard` jobs (all baked into stateful now)

### Fixes applied
- Python venv shebang resolution: `python3.11` symlinked into `/usr/local/bin` in runtime stage
- Cloudflared config: all hostnames updated from old container names to `localhost`
- `ALLOW_PRIVATE_CALLBACKS=true` on `runtime-api-local` — monolith callbacks are always localhost
- Bot network URLs: `BOT_REDIS_URL`, `BOT_MEETING_API_URL`, `BOT_TTS_URL`, `BOT_COOKIE_URL` now
  use `kioku-stateful` hostname (bot container is on `kioku-network`, ports not exposed to host)
- `runtime-api-runpod` skips autostart when `RUNPOD_API_KEY` is unset
- TTS voices dir changed to `/data/tts-voices` (avoids stale model files from old deployment)
- `COMPUTE_TYPE=int8` and `MODEL_SIZE` explicitly passed to bot containers via runtime-profiles.yaml
- SSH port remapped from 22 to 2222 (host port 22 in use)
- Stale build-path env vars removed from `.env.example`

### Previously done (issues #27–#37)
- RunPod backend, dashboard, MCP, runtime router, bot e2e, whisper=0 fixes — see git log

## Open Items

### Enable Google OAuth (one command when ready)

Everything is wired. Just need real credentials from Google Cloud Console:

1. Go to https://console.cloud.google.com → APIs & Services → Credentials
2. Create OAuth 2.0 Client ID (Web application)
3. Add redirect URI: `https://dashboard.kioku.chat/api/auth/callback/google`
4. Run on the server:
   ```bash
   cd ~/ws/kioku/deployment/docker
   ./setup-google-oauth.sh <client_id> <client_secret>
   ```

### Bot pool for meeting identity (issue #38)

Bots currently join meetings as a generic identity. Issue #38 tracks adding a pool of
pre-registered bot accounts for Google Meet, Zoom, Teams to avoid waiting-room friction.

### Authenticated bot self-leaves immediately (issue #43)

When the **Authenticated** toggle is ON in the dashboard Join form, the bot enters the meeting then
immediately self-leaves. Unauthenticated mode is unaffected. Root cause unknown — likely the cookie
service (`kioku-stateful:8099`) has no valid session cookies for the bot identity, triggering an
auth failure that causes an early exit. Check `docker logs meeting-<id>` for the exit reason.

### Bot cleanup when left alone (issue #41)

When the last human leaves a meeting, the bot should auto-exit rather than linger.
Needs an idle-detection heuristic in vexa-bot.

### Makefile (issue #42)

A `deployment/docker/Makefile` exists (untracked) — needs review and commit.

## Deploy Server

```
machine: 172.16.1.5
dir:     ~/ws/kioku/deployment/docker
stack:   docker compose -f docker-compose.stateful.yml
```

### Useful commands

```bash
# View all service logs live
docker logs -f kioku-stateful

# Check individual service
docker exec kioku-stateful tail -f /var/log/<service>.err

# Restart after entrypoint/config change (no rebuild needed for entrypoint fixes)
scp deployment/docker/entrypoint-stateful-runtime.sh machine:~/ep.sh
ssh machine "docker cp ~/ep.sh kioku-stateful:/entrypoint.sh && docker restart kioku-stateful"

# Pull updated profile (no restart needed — runtime-api hot-reloads via SIGHUP)
ssh machine "cd ~/ws/kioku && git pull && docker exec kioku-stateful kill -HUP \$(docker exec kioku-stateful pgrep -f 'port 8091')"

# Clean build cache after rebuilds
ssh machine "docker builder prune --all -f && docker image prune -a -f"
```

### Bot concurrency

| Resource | Total | Per bot (int8) | Safe cap |
|---|---|---|---|
| GPU VRAM (RTX 3070, 8 GB) | ~7.8 GB | large-v3-turbo ~1.5 GB | ~4–5 |
| RAM (32 GB) | ~28 GB free | 2.5 GB limit | ~11 |
| CPU (8 cores) | ~5–6 free | 1–1.5 cores | ~4 |

`LOCAL_BOT_THRESHOLD=3` in `.env` — bots beyond this overflow to RunPod (requires `RUNPOD_API_KEY`).

### Runtime router behaviour

| `USE_LOCAL_RESOURCE` | local bot count | Routes to    |
|---|---|---|
| `false`              | any             | RunPod (all) |
| `true`               | < threshold     | local Docker |
| `true`               | ≥ threshold     | RunPod       |

## Verify After Restart

```bash
curl https://dashboard.kioku.chat           # 200
curl https://meetings.kioku.chat/health     # {"message":"Welcome to the Vexa API Gateway"}
curl https://mcp.kioku.chat/health          # {"status":"ok"}
curl https://api.kioku.chat/health          # {"status":"ok"}
```

## GitHub Issue State

- `#27` closed: RunPod stateful path
- `#28` closed: stateless GPU path
- `#30` closed: dashboard + MCP moved and deployed
- `#31` closed: dashboard.kioku.chat live
- `#32` closed: runtime router implemented
- `#33` closed: bot joins meetings end-to-end
- `#34` closed: Google OAuth wired
- `#35` closed: whisper=0 fully fixed
- `#36` closed: Dockerfile.stateless module-build chain fixed
- `#37` closed: collector JWT mismatch fixed
- `#40` closed: docker images refactored — true stateful/stateless monolith
- `#38` open: bot pool for meeting identity
- `#41` open: bot cleanup when left alone
- `#42` open: Makefile for fresh install
- `#43` open: authenticated mode causes bot to self-leave immediately
