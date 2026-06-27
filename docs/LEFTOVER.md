# LEFTOVER

Last updated: 2026-06-27

## Current Status

CI is fully green. `ghcr.io/kioku-org/kioku-dashboard:latest` is published and ready to pull.

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
- **Fixed**: `vexa-runtime-api` service added to `docker-compose.stateless.yml` — was missing, causing bot spawning to silently fail (meeting-api was pointing `RUNTIME_API_URL` to itself)
- `deployment/docker/configs/runtime-profiles.yaml` — profiles config for the local runtime-api
- `RUNTIME_ORCHESTRATOR` env var: `docker` (default, local) or `runpod` (spawn on RunPod)
- Docker socket mount moved from `vexa-meeting-api` to `vexa-runtime-api` (where it belongs)

## Deploy Server Steps (Run on the Server)

```bash
cd deployment/docker

# 1. Pull updated images
docker compose -f docker-compose.stateless.yml pull kioku-dashboard

# 2. Start/update services (runtime-api is new — will be built from source)
docker compose -f docker-compose.stateless.yml up -d --build vexa-runtime-api kioku-dashboard

# 3. Reload cloudflared to pick up new tunnel routes (dashboard + mcp)
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

There is no global server-side cap in the current vexa code — the per-user limit is the mechanism.

## RunPod Overflow (Future)

To route overflow bots to RunPod when the local count is exhausted, a runtime-router
proxy service is needed between the meeting-api and the two runtime backends (local Docker +
RunPod). This is not yet implemented.

Short-term workaround: set `RUNTIME_ORCHESTRATOR=runpod` in `.env` to use RunPod for **all**
bots (no local spawning). This bypasses the 3-bot local cap entirely and bills RunPod per bot.

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
- `#30` open: dashboard+MCP in Kioku — both halves done; close after successful deploy
- `#31` open: publish dashboard.kioku.chat — image ready, pending deploy server steps above
