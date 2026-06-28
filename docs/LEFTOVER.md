# LEFTOVER

Last updated: 2026-06-28 (rev 2)

## Current Status

Dashboard is fully rebranded (commit `8d899b0`, CI building now).
RunPod test failure fixed (commit pending — venv symlink bug).
Server-side bot deployment failing — see "Bot Deployment Blocker" below.

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

## Pending / Blockers

### 1. RunPod venv symlink bug (JUST FIXED, needs CI)

**Root cause**: Stage 2 builds venv on `python:3.11-slim-bookworm` where Python is at
`/usr/local/bin/python3.11`. Stage 3 (Ubuntu 22.04 + deadsnakes) puts Python at
`/usr/bin/python3.11`. The copied venv has a broken `/opt/venv/bin/python3.11 →
/usr/local/bin/python3.11` symlink — all Vexa Python services fail to import.

**Fix** (committed, CI building): Added `ln -sf /usr/bin/python3.11 /usr/local/bin/python3.11`
in Stage 3 after the deadsnakes install. This satisfies the venv's expected path.

**Impact**: All Vexa Meeting API, API Gateway, Runtime API were returning HTTP 502 on RunPod.

### 2. Server bot deployment failing

Dashboard → API Gateway → Meeting API → Runtime Router → local runtime-api-local → spawns
`ghcr.io/kioku-org/kioku-stateless:latest` container via Docker socket.

**Three root causes fixed in scripts** (committed):
- `setup.sh` now explicitly pulls `ghcr.io/kioku-org/kioku-stateless:latest` (it was never pulled
  because it's spawned at runtime, not listed as a compose service)
- `setup.sh` now auto-fills empty `DOCKER_GID=` (from .env.example copy) by detecting the host GID
- `smoke-test.sh` had wrong container names (`kioku-postgres`/`kioku-qdrant` → `postgres`/`qdrant`,
  `kioku-vexa-mcp` → `kioku-mcp`); missing `kioku-vexa-runtime-api-local` and `kioku-runtime-router`
- `healthcheck.sh` now checks `kioku-vexa-runtime-api-local` and the bot image availability

**Still needs on server** — re-run setup to apply the fixes:
```bash
cd /home/growit/ws/kioku/deployment/docker
./scripts/setup.sh          # pulls bot image + fixes DOCKER_GID
./scripts/manage.sh restart # pick up new env
./scripts/healthcheck.sh    # verify bot image is present
```

If services are still failing after setup:
```bash
docker compose -f docker-compose.stateless.yml ps       # are all services running?
docker logs kioku-vexa-runtime-api-local --tail 50      # runtime-api errors?
docker logs kioku-runtime-router --tail 50              # router logs?
```

### 3. Google OAuth setup (not yet configured)

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
- `#31` open → partially done: dashboard.kioku.chat live but bot deployment broken + no Google OAuth yet
- `#32` open → ready to close: runtime router implemented and deployed
