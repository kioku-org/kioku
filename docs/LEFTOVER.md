# LEFTOVER

Last updated: 2026-06-29 (rev 6)

## Current Status

Pipeline fully verified end-to-end via FAKE_AUDIO test: audio → speakerManager → whisper → Redis →
collector → Postgres → dashboard all confirmed working. Root cause of `whisper=0` was a JWT signing
key mismatch (ADMIN_TOKEN changed, so collector silently dropped all segments from older bots).
New bots automatically get freshly-minted tokens and are unaffected. Real-meeting audio capture
(AudioWorklet → onAudio → queue) still needs one live test to confirm.

## What Is Done

- RunPod backend proven in CI (commits `7db105b`, `ebcffeb`)
- Dashboard code migrated into `services/dashboard` (issue #30 closed)
- `services/dashboard/Dockerfile` rewritten for the kioku repo context
- `build-dashboard` CI job added — builds and pushes to GHCR on every push to master
- `service-tests.yml` `dashboard-build` job passing (TypeScript + Next.js build verified)
- `kioku-dashboard` service added to `deployment/docker/docker-compose.stateless.yml`
- `dashboard.kioku.chat` + `mcp.kioku.chat` ingress added to `deployment/docker/cloudflared.yml` (live, gitignored)
- `cloudflared.yml.example` updated with both `dashboard.example.com` and `mcp.example.com` entries
- Dashboard + RunPod env vars added to `deployment/docker/.env.example`
- `services/dashboard/deploy/` created for standalone dashboard-only deployments
- **Fixed**: `vexa-runtime-api` service added to `docker-compose.stateless.yml` — was missing, causing bot spawning to silently fail
- `deployment/docker/configs/runtime-profiles.yaml` — profiles config for the local runtime-api
- Docker socket mount moved from `vexa-meeting-api` to `vexa-runtime-api`
- **MCP (issue #30)**: `services/mcp/` wired into compose as `kioku-mcp` using published image
- `build-mcp` CI job added — builds and pushes `ghcr.io/kioku-org/kioku-mcp:latest` on every push to master
- MCP `/health` endpoint added; CI integration job starts MCP and runs `parse_meeting_link` test against it
- `kioku mcp` CLI command now outputs both Hivemind MCP + Meetings MCP configs; 3 unit tests added
- `docs/mcp/overview.md` updated to document both MCPs and `kioku mcp` CLI usage
- **Runtime Router (issue #32)**: implemented with `USE_LOCAL_RESOURCE` + `LOCAL_BOT_THRESHOLD` overflow logic (closed)
- **Dashboard rebrand**: all user-visible Vexa→Kioku across 71 files; new Kioku logo SVGs created
- **Dashboard auth**: NextAuth with Google OAuth fully wired (auto-registers new users); direct email mode for dev
- **Stateful Dockerfile**: base changed to `nvidia/cuda:12.3.2-cudnn9-runtime-ubuntu22.04` for GPU-optional Ollama
- **Chrome revision fix** (commit `13830b7`): `Dockerfile.stateless` Stage 3 now copies chromium-1194 from
  `ts-builder` instead of running unversioned `npx playwright install` (which downloaded chromium-1228,
  mismatching `playwright-core 1.56.0` which expects revision 1194)
- **Transcription port fix** (commit `ce7c407`): all `TRANSCRIPTION_SERVICE_URL` references corrected
  from port 80 → 8000 (the FastAPI transcription service listens on 8000, not 80)
- **Bot joins meetings end-to-end**: bot successfully enters name, clicks Ask to Join, waits in
  waiting room, gets admitted, and per-speaker audio streams connect (issue #33 closed)
- **Dockerfile module-build fix (issue #36 closed)**: ts-builder now copies `services/vexa/modules/`
  and builds all `@vexa/*` packages in dep order before `npm install`; fixed playwright browser path;
  added `/modules` COPY to runtime stage
- **AudioContext fix**: `--autoplay-policy=no-user-gesture-required` added to `JOIN_BROWSER_ARGS`
- **whisper=0 investigation (issue #35)**: queue-poll pattern implemented and pipeline verified:
  - Browser: AudioWorklet pushes `{i, d}` chunks to `window.__vexaAudioQueue`
  - Node.js: 100ms poller drains queue via `page.evaluate`, calls `handlePerSpeakerAudioData` directly
  - **Root cause of no-transcription**: `ADMIN_TOKEN` changed between when meeting-100 was created
    and when collector verified its JWT — all segments were silently dropped with
    `"failed MeetingToken verification"`. New bots get fresh tokens; unaffected going forward.
  - **Pipeline verified**: `FAKE_AUDIO_FILE` mode added to `index.ts` — feeds a WAV file directly
    into `speakerManager` (bypassing browser), confirmed full path to dashboard works end-to-end
  - **Still needed**: one live meeting test to confirm AudioWorklet → onAudio → queue in real Chrome
  - Image: `kioku-stateless:0.11` on deploy server
- **Dashboard live (issue #31 closed)**: `dashboard.kioku.chat` serves real product dashboard;
  backends reachable; direct login enabled for trial use
- **Google OAuth wired (issue #34 closed)**: NextAuth + Google provider fully implemented;
  `NEXTAUTH_SECRET` and `NEXTAUTH_URL` set on server; setup script at
  `deployment/docker/setup-google-oauth.sh` — one command to activate once credentials are obtained

## Open Items

### Verify real-meeting audio capture

The full pipeline (speakerManager → whisper → dashboard) is confirmed. The remaining unknown is
whether real Google Meet audio flows through the AudioWorklet → onAudio → `__vexaAudioQueue` chain.

Start a fresh meeting, admit the bot, then stream its logs:
```bash
# Find the bot container name
ssh machine "docker ps --format '{{.Names}}' | grep meeting-"

# Watch for transcription activity
ssh machine "docker logs -f <container> 2>&1 | grep -E 'DRAFT|PUBLISH|LANGUAGE|onAudio|ERROR'"
```
If `[📝 DRAFT]` lines appear → audio capture works, issue #35 fully resolved.
If not → AudioWorklet onAudio chain is broken and needs further investigation.

### Enable Google OAuth (one command)

Everything is wired. Just need real credentials from Google Cloud Console:

1. Go to https://console.cloud.google.com → APIs & Services → Credentials
2. Create OAuth 2.0 Client ID (Web application)
3. Add redirect URI: `https://dashboard.kioku.chat/api/auth/callback/google`
4. Run on the server:
   ```bash
   cd /home/growit/ws/kioku/deployment/docker
   ./setup-google-oauth.sh <client_id> <client_secret>
   ```
   The script writes creds, disables open direct login, and restarts the dashboard.

### Internal Whisper on bot container (noise)

`/entrypoint.sh` always starts an internal Whisper GPU service before the Node.js bot. On the
deploy server the internal service is unused (bot points at `kioku-vexa-transcription-service`),
but it still spins up and either crashes (no CUDA) or wastes GPU memory (CUDA present but shared).

Fix: in `/entrypoint.sh`, skip the Python Whisper launch when `TRANSCRIPTION_SERVICE_URL` is
already set and reachable externally. Low priority — does not affect correctness.

## Server-Side Workarounds (temporary)

Bot image is locally built on the deploy server (not yet pushed to GHCR):
```
VEXA_BOT_IMAGE=kioku-stateless:0.11
```
Built at `/home/growit/ws/kioku` with:
```bash
docker build -f deployment/runpod/Dockerfile.stateless -t kioku-stateless:0.11 .
```
Once changes are merged and CI pushes `kioku-stateless:latest` to GHCR, revert to:
```bash
VEXA_BOT_IMAGE=ghcr.io/kioku-org/kioku-stateless:latest
```

## Deploy Server Steps

```bash
cd /home/growit/ws/kioku/deployment/docker

# Pull latest images (after CI builds on master push)
docker compose -f docker-compose.stateless.yml pull kioku-dashboard kioku-mcp
docker compose -f docker-compose.stateless.yml up -d --no-deps kioku-dashboard kioku-mcp

# Update bot image (after whisper=0 fix is merged to master and CI builds):
# 1. In .env: VEXA_BOT_IMAGE=ghcr.io/kioku-org/kioku-stateless:latest
# 2. docker compose -f docker-compose.stateless.yml up -d --no-deps vexa-runtime-api-local vexa-meeting-api
```

## Bot Concurrency Cap

```bash
# Get users
curl -s -H "X-Admin-API-Key: $VEXA_ADMIN_API_TOKEN" http://localhost:8057/admin/users

# Set bot cap
curl -X PATCH -H "X-Admin-API-Key: $VEXA_ADMIN_API_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"max_concurrent_bots": 3}' \
  http://localhost:8057/admin/users/{user_id}
```

## Runtime Router Behaviour

| `USE_LOCAL_RESOURCE` | local bot count | Routes to    |
|----------------------|-----------------|--------------|
| `false`              | any             | RunPod (all) |
| `true`               | < N             | local Docker |
| `true`               | ≥ N (overflow)  | RunPod       |

## After Deploy — Verify

```bash
curl -I https://dashboard.kioku.chat
curl -s https://dashboard.kioku.chat/api/health | python3 -m json.tool
curl -I https://mcp.kioku.chat/health
```

## GitHub Issue State

- `#27` closed: RunPod stateful path
- `#28` closed: stateless GPU path
- `#30` closed: dashboard + MCP moved and deployed
- `#31` closed: dashboard.kioku.chat live, direct login works
- `#32` closed: runtime router implemented and deployed
- `#33` closed: bot joins meetings; Chrome revision + transcription port fixed
- `#34` closed: Google OAuth fully wired; operator runs setup-google-oauth.sh with credentials
- `#35` open: whisper=0 — pipeline confirmed via FAKE_AUDIO; real-meeting audio capture still needs live test
- `#36` closed: Dockerfile.stateless module-build chain fixed
- `#37` open: collector silently drops segments when ADMIN_TOKEN rotated (MeetingToken verification fails)
