# LEFTOVER

Last updated: 2026-06-28 (rev 3)

## Current Status

Bot deployment working end-to-end on dev server: Chrome launches, bot navigates to Google Meet,
enters name, clicks "Ask to join", waits for admission. Transcription connects to external service.
Remaining: CI rebuild of `kioku-stateless:latest` with Chromium fix so `:chrome-fix` workaround
can be dropped. Google OAuth still needs Google Cloud Console credentials.

## What Is Done

- RunPod backend proven in CI (commits `7db105b`, `ebcffeb`)
- Dashboard code migrated into `services/dashboard` (issue #30 scope complete)
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
- **Runtime Router (issue #32)**: implemented with `USE_LOCAL_RESOURCE` + `LOCAL_BOT_THRESHOLD` overflow logic
- **Dashboard rebrand**: all user-visible Vexa→Kioku across 71 files; new Kioku logo SVGs created
- **Dashboard auth**: NextAuth with Google OAuth fully wired (auto-registers new users); direct email mode for dev
- **Stateful Dockerfile**: base changed to `nvidia/cuda:12.3.2-cudnn9-runtime-ubuntu22.04` for GPU-optional Ollama
- **Chrome revision fix** (commit `13830b7`): `Dockerfile.stateless` Stage 3 now copies chromium-1194 from
  `ts-builder` instead of running unversioned `npx playwright install` (which downloaded chromium-1228,
  mismatching `playwright-core 1.56.0` which expects revision 1194)
- **Transcription port fix** (commit `ce7c407`): all `TRANSCRIPTION_SERVICE_URL` references corrected
  from port 80 → 8000 (the FastAPI transcription service listens on 8000, not 80)
- **Bot joins meetings end-to-end**: bot successfully enters name, clicks Ask to Join, waits in
  waiting room, gets admitted, and transcription flows via external `kioku-vexa-transcription-service`

## Server-Side Workarounds (temporary)

On the dev server, bot image is pinned to a locally-built workaround image:
```
VEXA_BOT_IMAGE=ghcr.io/kioku-org/kioku-stateless:chrome-fix
```
The `:chrome-fix` image was built on the server with:
```bash
docker build --platform linux/amd64 \
  -t ghcr.io/kioku-org/kioku-stateless:chrome-fix \
  -f deployment/runpod/Dockerfile.stateless .
```
Once CI rebuilds `kioku-stateless:latest` from the Dockerfile fix (commit `13830b7`), revert:
```bash
# In deployment/docker/.env on server:
VEXA_BOT_IMAGE=ghcr.io/kioku-org/kioku-stateless:latest
# Then restart:
docker compose -f docker-compose.stateless.yml up -d --no-deps vexa-runtime-api-local vexa-meeting-api
```

## Pending / Blockers

### 1. Internal Whisper in bot container crashes (harmless)

The outer entrypoint (`entrypoint-bot-runtime.sh`) starts an internal Whisper GPU service.
On the local server (no GPU / CUDA driver mismatch), it crashes with:
```
CUDA failed with error CUDA driver version is insufficient for CUDA runtime version
```
This is **harmless** — the bot falls back to the external `vexa-transcription-service` container
which runs fine. But it pollutes logs and could be fixed by skipping the internal service when
`TRANSCRIPTION_SERVICE_URL` is already set externally.

### 2. Google OAuth setup (not yet configured) — issue #34

The code is fully implemented (NextAuth in `services/dashboard/src/app/api/auth/[...nextauth]/route.ts`).
When `GOOGLE_CLIENT_ID` and `GOOGLE_CLIENT_SECRET` are set, the "Continue with Google" button
appears automatically. Users who sign in with Google get auto-registered.

Steps for the server:
1. Go to https://console.cloud.google.com → APIs & Services → Credentials
2. Create OAuth 2.0 Client ID (Web application)
3. Authorized redirect URI: `https://dashboard.kioku.chat/api/auth/callback/google`
4. Add to `.env` on server:
   ```
   GOOGLE_CLIENT_ID=your-client-id.apps.googleusercontent.com
   GOOGLE_CLIENT_SECRET=your-secret
   VEXA_ALLOW_DIRECT_LOGIN=false   # disable open email login for production
   ```
5. Restart dashboard: `docker compose -f docker-compose.stateless.yml restart kioku-dashboard`

For dev/testing `VEXA_ALLOW_DIRECT_LOGIN=true` means typing any email logs you in (no verification).

## Deploy Server Steps (after CI builds)

```bash
cd /home/growit/ws/kioku/deployment/docker

# Pull new dashboard image (CI takes ~10 min after push to master)
docker compose -f docker-compose.stateless.yml pull kioku-dashboard

# Restart
docker compose -f docker-compose.stateless.yml up -d --no-deps kioku-dashboard

# Also pull stateless bot image (used by runtime-api-local to spawn Chrome bots)
docker pull ghcr.io/kioku-org/kioku-stateless:latest
# Then update VEXA_BOT_IMAGE=ghcr.io/kioku-org/kioku-stateless:latest in .env
# and restart meeting-api + runtime-api-local
```

## Bot Concurrency Cap

After users register, set their bot cap via admin-api:

```bash
# Get list of users
curl -H "Authorization: Bearer $VEXA_ADMIN_API_TOKEN" http://localhost:8057/admin/users

# Patch a user's bot limit
curl -X PATCH -H "Authorization: Bearer $VEXA_ADMIN_API_TOKEN" \
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

Set `USE_LOCAL_RESOURCE=true` and `LOCAL_BOT_THRESHOLD=3` in `.env` for the server.

## After Deploy — Verify

```bash
curl -I https://dashboard.kioku.chat
curl -s https://dashboard.kioku.chat/api/health | jq .
curl -I https://mcp.kioku.chat/health
```

Expected: `"status": "ok"` with `googleOAuth.configured: true` if OAuth is set up.

## GitHub Issue State

- `#27` closed: RunPod stateful path
- `#28` closed: stateless GPU path
- `#30` open → ready to close: dashboard + MCP moved and deployed
- `#31` open → partially done: dashboard.kioku.chat live, bot works, Google OAuth pending
- `#32` open → ready to close: runtime router implemented and deployed
- `#33` open → **FIXED**: bot now joins meetings; Chrome revision + transcription port fixed
- `#34` open: Google OAuth — needs Google Cloud Console credentials
