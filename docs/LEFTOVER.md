# LEFTOVER

Last updated: 2026-07-02 (rev 14)

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

**16 visible commands** (as of `cli/v0.1.2-dev.2`, current latest — see `## GitHub Issue State` #56/#58/#59 for what changed):

```
signin     signout    whoami     token
search     docs       meetings   meeting
transcript meet       cal        keys
mcp        upgrade    completions
```

`docs` now folds in upload/delete (`docs`, `docs <path>`, `docs --delete <id>`);
`keys` folds in create/delete (`keys`, `keys --create`, `keys --delete <id>`).
Hidden (still functional, deliberately not orphaned): `auth-token`, `register-admin`.
The old `sessions`/`session-create`/`session-get`/`session-delete`/`send`/`messages`
hidden commands were removed as orphaned (superseded by the MCP `session`/`meeting` tools).

**Releases:** `cli/v0.1.2-dev.2` is current (linux x86_64 only). Install with
`export KIOKU_VERSION=cli/v0.1.2-dev.2 && curl -fsSL https://kioku.chat/install.sh | sh`
— **do not** use `KIOKU_VERSION=... curl ... | sh` on one line, the env var only
applies to `curl`, not the piped `sh` (see #62). Plain `curl ... | sh` with no
`KIOKU_VERSION` is currently broken for everyone (#62, also open).

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

- **#45** — produce Figma design for dashboard — **in progress, see below**
- **#44** — implement Figma design into Next.js dashboard (blocked by #45)

**Figma file:** https://www.figma.com/design/HVIghjV1LG91z07MjmCKQo/Kioku

Built via Figma MCP directly from `services/dashboard` source (not live-fetched — no browser
tool was available at build time). File has two pages:
- **Components** — design tokens (colors/radius converted from the shadcn `globals.css` oklch
  theme, light mode only; Geist Sans/Mono type), Button/Badge/Input/Avatar/Card/Sidebar-nav-item
  components, and an "App Shell" template (Header + Sidebar) that every screen instances + detaches.
- **Screens** — 12 frames: Login, Meetings, Meeting Detail, Profile/API Keys, Workspace, Settings,
  MCP Setup, Agent chat, Webhooks, Tracker, Admin Users, Admin Bots. Sidebar nav click-through wired
  (70 links) plus meetings-row → Meeting Detail and Login → Meetings; Login is the flow start point.

**Not done yet:**
- Dark mode variables (only light mode tokens exist in Figma so far)
- Pixel-diffing against the live app — installed `playwright` MCP server (user scope, via
  `claude mcp add --scope user playwright -- npx -y @playwright/mcp@latest`) to browse
  `dashboard.kioku.chat` for this, but it was added mid-session so the running session couldn't
  load it. **Next session should have it available automatically** — use it to screenshot the
  live dashboard and reconcile spacing/copy/colors against the Figma frames above before closing #45.
- Only the primary/default state of complex pages (Meeting Detail, Agent, Workspace) was built —
  no loading/error/editing states.

## Bugfix SOP

Standard pipeline for any bug found (in this repo or on the live deploy server). Follow all
steps in order — don't skip the issue or the live verification.

1. **Confirm root cause before filing.** Read the actual code path, don't guess. Reproduce if
   possible (a small standalone script/curl beats a hunch).
2. **File a GitHub issue** (`gh issue create --repo kioku-org/kioku`) with: symptom, root cause
   with `file:line` references, and the fix approach. Do this even for small fixes — it's the
   paper trail for the "Fixes #N" commit trailer below and for `## GitHub Issue State`.
3. **Isolate in a worktree** off local `master` (not `origin/main` — the two have diverged;
   `master` is the branch actually deployed). `EnterWorktree` defaults to branching from
   `origin/<default-branch>`, which may lack recent work — if so, branch manually:
   `git worktree add .claude/worktrees/<name> master -b <name>` then
   `EnterWorktree({ path: ... })`.
4. **Implement the fix.** Minimal, targeted — no drive-by refactors.
5. **Validate before committing:**
   - dashboard (TS): symlink `node_modules` from `services/dashboard` into the worktree copy,
     run `node scripts/generate-release-version.js` (needs `NEXT_PUBLIC_VEXA_OSS_VERSION` set,
     e.g. `=0.1.0`, if no git tag/VERSION is resolvable), then `npx tsc --noEmit -p tsconfig.json`
     and `npx eslint <changed files>`.
   - python (admin-api/meeting-api/etc): `python3 -m py_compile <changed files>` at minimum.
   - Where feasible, reproduce the exact bug logic in a throwaway `node -e` / script to prove the
     fix resolves it (e.g. the cookie-decode bug in #54 was reproduced standalone before trusting
     the fix).
6. **Commit** with a body explaining root cause (not just what changed) and a `Fixes #N` trailer
   so GitHub auto-closes the issue on merge to `master`.
7. **Merge to local `master` and sync:**
   ```bash
   git -C <repo-root> fetch origin master:refs/remotes/origin/master
   git -C <repo-root> log --oneline master..origin/master   # check for divergence
   git -C <repo-root> merge --ff-only <branch>               # or `rebase origin/master` first if it diverged
   git -C <repo-root> push origin master
   ```
8. **Clean up the worktree:** `git worktree remove <path> --force && git branch -D <branch>`.
9. **Deploy to the dev/production server** (see `## Deploy Server` below) — pull the code and
   **build from source** (not `docker compose pull`, unless explicitly told otherwise), recreate
   the container, and always chain a prune:
   ```bash
   ssh machine "cd ~/ws/kioku && git pull --ff-only origin master"
   ssh machine "cd ~/ws/kioku/deployment/docker && docker compose -f docker-compose.stateful.yml build kioku-stateful && docker compose -f docker-compose.stateful.yml up -d kioku-stateful && docker image prune -f"
   ```
10. **Verify live**, not just "container is running" — hit the actual endpoint/flow the bug was
    in (curl the API route with a real cookie/token round-trip, not just a health check). Docker's
    own healthcheck for `kioku-stateful` is known-broken (checks `/health` on :8056, which 404s —
    pre-existing, unrelated to app health) so don't use container health status as your signal.
11. **Comment on the issue** with what was fixed, the deployed commit SHA, and the live
    verification output. Let `Fixes #N` auto-close it.

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
- `#45` open: UI redesign — Figma design phase, 12/12 screens drafted, needs live-app pixel review
- `#44` open: UI redesign — implement Figma into dashboard (blocked by #45)
- `#47` open: Hivemind integration (feat/hivemind) — ready for PR review
- `#48` open: CLI binary distribution — multi-platform build workflow needed; install.sh blocked on kioku-web
- `#53` closed: admin login always failed — `admin-verify` threw on missing `JWT_SECRET`; fixed with fallback chain, deployed and verified
- `#54` closed: admin API proxy always 401'd — `admin/[...path]` route decoded the signed cookie wrong; sign/verify logic extracted to shared `lib/admin-session.ts`, deployed and verified
- `#55` closed: profile page's max-bots showed "—" — `/api/auth/me` dropped `max_concurrent_bots` (gateway returns it as `max_concurrent`); also standardized the free-tier default from a hardcoded 3 to 1 across 3 places. Deployed; code-verified, live click-through as regular user still pending (no plaintext user API key available)
- `#56` closed: CLI subcommand enhancements — `--json` global flag, `search --limit`, `kioku meeting`/`kioku transcript` (new Hivemind REST routes reusing the MCP tools' repo calls), `kioku completions`, removed orphaned agent-chat commands. Also consolidated `docs`/`upload`/`doc-delete` → `docs`/`docs <path>`/`docs --delete` and `keys`/`key-create`/`key-delete` → `keys`/`keys --create`/`keys --delete`. Deployed, live-verified against production with a real session.
- `#57` open: e2e test coverage across the whole system — deliberately not started this session (explicit user instruction to hold off)
- `#58` closed: `kioku meet` (join/list/kill bots) — 2 new Hivemind routes (`GET /vexa/bots/status`, `DELETE /vexa/bots/:platform/:id`) plus CLI wiring. Live-verified: `kioku meet` successfully round-tripped through the full #60 credential chain (lazy per-user Vexa token provisioning fired correctly, matched the existing Vexa user by email).
- `#59` open: `kioku cal` (list Google Calendar meetings today/`--week`/`--date`) — implemented (separate direct CLI↔Google PKCE OAuth flow, own Desktop-app OAuth client, distinct from dashboard sign-in). `KIOKU_GOOGLE_CALENDAR_CLIENT_ID`/`_SECRET` provisioned and wired into `docker-compose.stateful.yml` + the dev server's `.env`. Not yet exercised against the live Google Calendar API (that requires actually completing the browser consent flow) — left open until that's done.
- `#60` closed: Hivemind (`cmp_`/`kioku_` bcrypt keys, schema `hivemind`) ↔ Vexa (`api_tokens` plaintext, schema `vexa`) credential unification. Shipped: per-user Vexa token lazy-provisioning (`services/hivemind/src/handlers/vexa.rs`) replacing the single shared `vexa_admin_token`; `cmp_` → `kioku_` key prefix (backward compatible); `tier` column on `vexa.users`; MCP unification (`services/mcp` now exchanges any Kioku credential for the caller's Vexa token via new `GET /vexa/token`, so one credential works for both MCP servers). Deployed, live-verified via DB inspection and a real `kioku meet` call.
- `#61` closed: admin-api crashed on startup during the #60 deploy — `schema_sync._col_default_sql()` emitted unquoted string `DEFAULT` values (`DEFAULT free` instead of `DEFAULT 'free'`), invalid SQL. Fixed with proper quoting + escaping, regression-tested, redeployed clean.
- `#62` open: `install.sh`'s default (no `KIOKU_VERSION`) path 404s — every CLI release ever published is a GitHub pre-release, so `releases/latest` (what the script falls back to) never resolves. Needs either a real non-prerelease release cut, or the installer's fallback logic changed to consider prereleases via the GitHub API. The `keys` (plural, not singular `key`) naming portion of this issue is fixed and shipped (`cli/v0.1.2-dev.2`).
- `#66` closed: `kioku signin` now chains Google Calendar consent into the same OAuth round trip when signing in via Google — no more separate `kioku cal` connect step later. Superseded the original design (a second, CLI-embedded Desktop-app Google OAuth client, `KIOKU_GOOGLE_CALENDAR_CLIENT_ID`/`_SECRET`) with a dashboard-mediated one: a second NextAuth provider (`google-calendar`, wider scope, CLI-only — the web `/login` page is unaffected) reuses the dashboard's existing confidential Google client, and a new `/api/cli/google-calendar/refresh` route lets the CLI refresh its access token without ever holding the client secret. `KIOKU_GOOGLE_CALENDAR_CLIENT_ID`/`_SECRET` removed from `docker-compose.stateful.yml` and `.env.example` — no longer needed anywhere.
- `#65` — tracking issue for the design above, closed by #66.
- `#67` open: found while building #66 — `api/calendar/oauth/{start,complete}` (a separate, older, *unrelated* Google Calendar OAuth scaffold) is dead code (nothing in the dashboard UI calls it) with a real auth gap (no session/bearer check, takes `userEmail` straight from an unverified request body). Not fixed — either wire it up properly or delete it.
