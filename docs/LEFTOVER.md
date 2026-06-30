# LEFTOVER

Last updated: 2026-07-01 (rev 11)

## Current Status

True stateful/stateless architecture deployed and running on the deploy server. All 18 supervisord
processes in `kioku-stateful` come up cleanly. Bot containers spawn on-demand via Docker socket.
`kioku-stateless` image builds from `deployment/docker/Dockerfile.stateless`. CI pushes both images
to GHCR on every push to master.

`feat/hivemind` branch is active with Hivemind MCP integration, CLI OAuth signin, and GitHub OAuth.
Not yet merged to main — needs final testing pass before PR.

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

### Hivemind MCP integration (feat/hivemind, issue #47)

**MCP tools** — all live at `POST /mcp` (Streamable HTTP, requires `Mcp-Session-Id` header):

| Tool | What it does |
|---|---|
| `search` | Semantic search across knowledge base; `company_id` from JWT |
| `meetings` | List all meetings for company |
| `meeting_get` | Get meeting details by id |
| `transcript` | Get meeting transcript; `company_id` from JWT |
| `documents` | List uploaded documents |
| `document_delete` | Delete a document by id |
| `session` | Ingest a coding/work session — chunks content via paragraph-aware splitter into `coding_sessions` + `knowledge_chunks` + Qdrant |
| `meeting` | Ingest a raw meeting transcript directly |

**Paragraph-aware chunking** (`services/hivemind/src/services/knowledge.rs:307`):
- `split_text_paragraphs(text, max_words=400)` — splits on `\n\n` first, word-windows only oversized paragraphs, carries last paragraph as overlap context

**DB migration** `005_coding_sessions.sql`:
- `coding_sessions` table (uuid, company_id, user_id, title, summary, decisions, tags, date)
- `knowledge_chunks.session_id` FK added

**Other Hivemind fixes done:**
- Vexa → Hivemind transcript pipeline (`push_to_hivemind` in `post_meeting.py`)
- Qdrant gRPC fix (qdrant-client 1.x needs `grpc_port: 6335`)
- MCP lazy provision — existing users without hivemind token get one on next JWT refresh
- `search` + `transcript` tools resolve `company_id` from JWT (not required as a tool arg)
- `INTERNAL_API_SECRET` used for service-to-service provision calls (set in `.env`)

### CLI (feat/hivemind, issue #48)

**Binary:** `services/cli/` workspace with crates `cc-cli`, `cc-auth`, `cc-kioku`, `cc-upgrade`

**14 visible commands** (as of dev.5):

```
signin     signout    whoami     token
search     upload     docs       doc-delete
meetings
keys       key-create key-delete
mcp        upgrade
```

Hidden (still functional): `sessions`, `session-create`, `session-get`, `session-delete`, `send`, `messages`, `auth-token`, `register-admin`

**`kioku signin` OAuth flow:**
1. Animated left/right provider selector (crossterm) — Google / GitHub, navigate with `← →` / `h l`
2. Opens browser to `https://dashboard.kioku.chat/cli-auth?port=<random>&state=<uuid>&provider=<google|github>`
3. `/cli-auth` (route handler, no dashboard layout) checks session → provisions Hivemind JWT → 302 to `http://localhost:<port>/callback?token=...`
4. CLI receives callback, validates CSRF state, saves `AuthFile`

**`kioku upgrade`** — merged check + upgrade: prints "already up to date" if current, otherwise upgrades.

**`register-admin`** — hidden from `--help`; errors with a friendly message if called against `api.kioku.chat`.

**`DEFAULT_SERVER_URL`** = `https://api.kioku.chat`
**`DEFAULT_DASHBOARD_URL`** = `https://dashboard.kioku.chat`

**Install script** (`docs/install.sh`):
- Braille spinner animation, ANSI colors, TTY detection
- Auto-detects OS/arch (linux/macos × x86_64/aarch64)
- Installs to `/usr/local/bin` or `~/.local/bin`
- `KIOKU_VERSION=cli/v0.1.0-dev.5 curl -fsSL https://kioku.chat/install.sh | sh` for dev builds
- Needs to be copied to `kioku-web/install.sh` (blocked — kioku-web not yet pushed from Windows)

**GitHub releases** (all pre-release, linux x86_64 only so far):
- `cli/v0.1.0` — initial release
- `cli/v0.1.0-dev.1` through `cli/v0.1.0-dev.5` — iterated during feat/hivemind

**Missing:** multi-platform release workflow — `release-cli.yml` GitHub Actions to build all 4 targets (linux x86_64/aarch64, macos x86_64/aarch64) automatically on `cli/vX.Y.Z` tag push. Currently building and uploading manually.

### Dashboard (feat/hivemind)

- **GitHub OAuth** added to NextAuth (`services/dashboard/src/app/api/auth/[...nextauth]/route.ts`):
  - `GithubProvider` enabled when `GITHUB_CLIENT_ID` + `GITHUB_CLIENT_SECRET` set
  - `signIn` callback handles `google`, `azure-ad`, and `github` providers
  - `GITHUB_CLIENT_ID` / `GITHUB_CLIENT_SECRET` wired into `docker-compose.stateful.yml`
  - Credentials stored in server `.env` (set up 2026-06-30)
- **GitHub button** on login page (`services/dashboard/src/app/login/page.tsx`):
  - Shows when `healthStatus.checks.githubOAuth.configured === true`
  - Official GitHub Octocat SVG mark
- **`/cli-auth` route handler** (`services/dashboard/src/app/cli-auth/route.ts`):
  - Route handler (not page component) → no dashboard layout/nav
  - Validates port + CSRF state, gets session, decodes existing `hivemindToken` from JWT or re-provisions
  - 302 redirect to `http://localhost:<port>/callback?token=...&state=...&user_id=...&email=...&name=...&company_id=...&role=...`
- **Health API** (`/api/health`) now reports `githubOAuth.configured`

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

### Fixes applied (historical)
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

## Open Items

### PR feat/hivemind → main (issue #47)

All Hivemind work is on `feat/hivemind`. Before merging:
- [ ] Verify `session` MCP tool end-to-end (ingest + search retrieval)
- [ ] Verify `kioku signin` → GitHub OAuth flow end-to-end on production
- [ ] Confirm `005_coding_sessions.sql` migration runs cleanly on existing prod DB
- [ ] PR review + merge

### CLI multi-platform release workflow (issue #48)

Need `.github/workflows/release-cli.yml` triggered on `cli/vX.Y.Z` tag:
- Build targets: `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`, `x86_64-apple-darwin`, `aarch64-apple-darwin`
- Upload all 4 tarballs to GitHub Release
- Currently only linux x86_64 is built manually

### CLI distribution via install.sh (issue #48)

`docs/install.sh` is ready. Needs to be copied to `kioku-web/` repo:
- Blocked until kioku-web repo is pushed from Windows
- Once done: `https://kioku.chat/install.sh` will serve it

### Bot pool for meeting identity (issue #38)

Bots currently join meetings as a generic identity. Issue #38 tracks adding a pool of
pre-registered bot accounts for Google Meet, Zoom, Teams to avoid waiting-room friction.

### Authenticated bot self-leaves immediately (issue #43)

When the **Authenticated** toggle is ON in the dashboard Join form, the bot exits with
`self_initiated_leave` (exit code 1). Root cause: cookie service (`kioku-stateful:8099`) has no
stored Google session cookies. Blocked by issue #38.

**Workaround:** use unauthenticated mode (toggle OFF) — bot joins via Ask to Join waiting room.

### Bot cleanup when left alone (issue #41)

When the last human leaves a meeting, the bot should auto-exit. Needs idle-detection in vexa-bot.

### Makefile (issue #42)

`deployment/docker/Makefile` exists (untracked) — needs review and commit.

### UI redesign (issues #45 → #44)

- **#45** — produce Figma design for dashboard
- **#44** — implement Figma design into Next.js dashboard (blocked by #45)

No Figma link or design assets yet.

## Deploy Server

```
ssh:   ssh machine
dir:   ~/ws/kioku/deployment/docker
stack: docker compose -f docker-compose.stateful.yml
```

### Useful commands

```bash
# View all service logs live
docker logs -f kioku-stateful

# Check individual service
docker exec kioku-stateful tail -f /var/log/<service>.err

# Rebuild + redeploy (after code changes)
cd ~/ws/kioku && git pull && cd deployment/docker
docker compose -f docker-compose.stateful.yml build kioku-stateful
docker compose -f docker-compose.stateful.yml up -d

# Restart after entrypoint/config change (no rebuild needed)
scp deployment/docker/entrypoint-stateful-runtime.sh machine:~/ep.sh
ssh machine "docker cp ~/ep.sh kioku-stateful:/entrypoint.sh && docker restart kioku-stateful"

# Pull updated profile (no restart — runtime-api hot-reloads via SIGHUP)
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
curl https://api.kioku.chat/health | jq .   # hivemind health check
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
- `#45` open: UI redesign — Figma design phase
- `#44` open: UI redesign — implement Figma into dashboard (blocked by #45)
- `#47` open: Hivemind integration (feat/hivemind) — ready for PR review
- `#48` open: CLI binary distribution — multi-platform build workflow needed; install.sh blocked on kioku-web
