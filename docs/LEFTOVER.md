# LEFTOVER

Last updated: 2026-06-27

## Current Status

CI is fully green. `ghcr.io/kioku-org/kioku-dashboard:latest` and `ghcr.io/kioku-org/kioku-mcp:latest` are published and ready to pull.

The stateless docker-compose is now complete end-to-end: dashboard, MCP, and bot spawning all wired correctly.

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
- **MCP (issue #30)**: `services/mcp/` wired into compose as `kioku-mcp` using published image — replaces the vexa submodule's MCP
- `build-mcp` CI job added — builds and pushes `ghcr.io/kioku-org/kioku-mcp:latest` on every push to master
- MCP `/health` endpoint added; CI integration job starts MCP and runs `parse_meeting_link` test against it
- `kioku mcp` CLI command now outputs both Hivemind MCP + Meetings MCP configs; 3 unit tests added
- `docs/mcp/overview.md` updated to document both MCPs and `kioku mcp` CLI usage

## Deploy Server Steps (Run on the Server)

```bash
cd deployment/docker

# 1. Pull updated images
docker compose -f docker-compose.stateless.yml pull kioku-dashboard kioku-mcp

# 2. Start/update services
docker compose -f docker-compose.stateless.yml up -d --build vexa-runtime-api kioku-dashboard kioku-mcp

# 3. Reload cloudflared to pick up tunnel routes (dashboard + mcp)
docker restart kioku-cloudflared
```

DNS (only needed once if CNAMEs don't exist yet):
```bash
cloudflared tunnel route dns 1c11ebdd-f78a-4078-a780-74ecd1f73d56 dashboard.kioku.chat
cloudflared tunnel route dns 1c11ebdd-f78a-4078-a780-74ecd1f73d56 mcp.kioku.chat
```

Ensure `.env` on the server has `NEXTAUTH_SECRET` set (required for dashboard auth).

## Bot Concurrency Cap (3 local)

The bot limit is per-user, enforced by the admin-api field `max_concurrent_bots`.

After users register on the dashboard, set the cap via the admin-api:

```bash
# Get list of users
curl -H "Authorization: Bearer $VEXA_ADMIN_API_TOKEN" http://localhost:8001/admin/users

# Patch a user's bot limit (user_id from above)
curl -X PATCH -H "Authorization: Bearer $VEXA_ADMIN_API_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"max_concurrent_bots": 3}' \
  http://localhost:8001/admin/users/{user_id}
```

## Runtime Router (issue #32) — NOT YET IMPLEMENTED

Tracks: `USE_LOCAL_RESOURCE` (bool) + `LOCAL_BOT_THRESHOLD` (int N).

### Behaviour

| `USE_LOCAL_RESOURCE` | local bot count | Routes to |
|---|---|---|
| `false` | any | RunPod (all bots) |
| `true` | < N | local Docker |
| `true` | ≥ N (overflow) | RunPod |

### Design

New service: `services/runtime-router/` (FastAPI, ~100 lines).

- Two runtime-api instances in compose:
  - `vexa-runtime-api-local` — `ORCHESTRATOR_BACKEND=docker`, mounts Docker socket
  - `vexa-runtime-api-runpod` — `ORCHESTRATOR_BACKEND=runpod`, needs `RUNPOD_API_KEY`
- `kioku-runtime-router` (port 8090) sits between `vexa-meeting-api` and the two backends
  - `vexa-meeting-api` points `RUNTIME_API_URL=http://kioku-runtime-router:8090`
  - On `POST /bots`: checks in-memory local bot count vs `LOCAL_BOT_THRESHOLD`; routes to local or RunPod accordingly; increments/decrements counter on success
  - Tracks per-bot backend origin by `platform:meeting_id` key so stop/delete goes to the right backend
  - All other paths forwarded to local backend (or RunPod if `USE_LOCAL_RESOURCE=false`)
  - `GET /health` returns current count + config for observability

### New env vars (`.env.example`)

```env
# Runtime routing
USE_LOCAL_RESOURCE=true          # false = RunPod for all bots
LOCAL_BOT_THRESHOLD=3            # max local concurrent bots before overflow to RunPod
```

### Compose changes needed

- Rename `vexa-runtime-api` → `vexa-runtime-api-local` (docker socket stays here)
- Add `vexa-runtime-api-runpod` (same image, `ORCHESTRATOR_BACKEND=runpod`, no socket mount)
- Add `kioku-runtime-router` service
- Update `vexa-meeting-api` env: `RUNTIME_API_URL=http://kioku-runtime-router:8090`

### E2E test on RunPod (no deployment server)

Set `USE_LOCAL_RESOURCE=false` in `.env` — router sends everything to RunPod, local runtime-api idles harmlessly.

## After Deploy — Verify

```bash
curl -I https://dashboard.kioku.chat
curl -s https://dashboard.kioku.chat/api/health | jq .
curl -I https://mcp.kioku.chat/health
```

Expected health response: `"status": "ok"` or `"degraded"` (degraded is fine if SMTP/OAuth not configured — direct login still works).

## GitHub Issue State

- `#27` closed: RunPod stateful path
- `#28` closed: stateless GPU path
- `#30` open: dashboard+MCP in Kioku — dashboard done, MCP wired; close after successful deploy
- `#31` open: publish dashboard.kioku.chat — image ready, pending deploy server steps above
- `#32` open: runtime router (`USE_LOCAL_RESOURCE` + `LOCAL_BOT_THRESHOLD`) — design above, not yet implemented
