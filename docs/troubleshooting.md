---
title: "Troubleshooting"
description: "Fixes for common Kioku issues."
---

## Bot won't join the meeting

**Symptom**: POST /bots succeeds but bot never appears in the meeting.

1. Check bot container is running:
   ```bash
   docker ps | grep stateless
   ```

2. Check bot logs:
   ```bash
   docker logs <container_id> --tail 100
   ```

3. Verify `VEXA_PUBLIC_URL` is set to a URL the bot container can actually reach — not `localhost` (bot container can't reach the host's localhost).

4. On Docker Compose, bot containers reach the stateful services via container name on `kioku-network`. Verify:
   ```bash
   docker network inspect kioku-network
   ```

## "callback_url cannot target localhost"

runtime-api rejects bot requests whose callback URL points to localhost. Fix: set `ALLOW_PRIVATE_CALLBACKS=true` in the runtime-api-local supervisor environment inside `entrypoint-stateful-runtime.sh`, then restart the container.

## Dashboard shows "Server error"

1. Check Hivemind is running: `curl http://localhost:9100/health`
2. Check dashboard can reach Hivemind: dashboard makes server-side calls to Hivemind — both must be on the same network
3. Check `NEXTAUTH_URL` matches the URL you're accessing the dashboard from

## Transcription is silent / empty transcript

1. Verify faster-whisper loaded: check bot container logs for `Model loaded`
2. Check `COMPUTE_TYPE` is compatible with your GPU — use `int8` for broad compatibility
3. Verify audio is being captured: bot logs show `audio frames received`
4. If using CPU: transcription may lag behind real time; the transcript appears after the meeting ends

## Redis connection timeout (bots)

Bot containers connect to Redis using `BOT_REDIS_URL`. Common mistake: using the host's public IP when the bot container is on `kioku-network` and should use the container name.

Correct (Docker Compose):
```bash
BOT_REDIS_URL=redis://:${REDIS_PASSWORD}@kioku-stateful:6379/0
```

Correct (RunPod overflow — bot is remote):
```bash
BOT_REDIS_URL=redis://:${REDIS_PASSWORD}@<your-server-public-ip>:6379/0
```

## Meeting ends but transcript never appears in search

1. Check meeting-api logs for the exit callback:
   ```bash
   docker exec kioku-stateful supervisorctl tail -f meeting-api
   ```
2. Check Hivemind logs for the `/meetings` ingest call
3. Check Ollama is running and responsive:
   ```bash
   curl http://localhost:11434/api/embeddings -d '{"model":"nomic-embed-text-v2-moe","prompt":"test"}'
   ```

## Authenticated bot immediately self-leaves

The bot exits with `self_initiated_leave` because no browser cookies are stored for the user. Authenticated mode requires pre-captured Google/Zoom session cookies in the cookie service (port 8099). This workflow is not yet fully supported — see [issue #38](https://github.com/kioku-org/kioku/issues/38). Use non-authenticated mode (toggle OFF) in the meantime.

## kioku-stateless image not found

```
Unable to find image 'ghcr.io/kioku-org/kioku-stateless:latest' locally
```

Pull it explicitly:
```bash
docker pull ghcr.io/kioku-org/kioku-stateless:latest
```

This happens after `docker image prune -a` removes unused images. If it happens repeatedly, consider keeping one bot running (or use `docker image pull` in a cron job).

## All supervisord processes show FATAL

Common causes:
1. Missing required env var — check the container logs: `docker logs kioku-stateful | head -100`
2. Port conflict — another process on the host took a needed port before the container started
3. Missing GPU — if `COMPUTE_TYPE=cuda` but no GPU is visible, processes that require it will fail. Check: `docker exec kioku-stateful nvidia-smi`

## supervisorctl: no such file or socket

supervisord starts with `-c /etc/supervisor/conf.d/kioku.conf` which doesn't include a `[unix_http_server]` section. `supervisorctl` won't work.

Workarounds:
- Restart a process: `docker exec kioku-stateful kill -HUP <pid>`
- Get process status: `docker exec kioku-stateful cat /var/run/supervisor.status` (if configured)
- Restart the whole container: `docker restart kioku-stateful`
- For runtime-profiles.yaml changes only: send SIGHUP to runtime-api-local — it hot-reloads profiles without restart

## Dashboard connects to `meetings.kioku.chat` instead of my own server

`docker-compose.stateful.yml` falls back to `VEXA_PUBLIC_API_URL=https://meetings.kioku.chat`
if you don't set it explicitly. This makes a self-hosted dashboard's browser-side WebSocket
connect to Kioku's production domain instead of your own server. Fix: set
`VEXA_PUBLIC_API_URL` explicitly in your `.env` to your own public URL.

## Cloudflare Tunnel — 502 Bad Gateway

1. Check cloudflared is running inside the container:
   ```bash
   docker exec kioku-stateful supervisorctl status cloudflared
   ```
2. Verify `cloudflared.yml` ingress hostnames point to `localhost:PORT`, not container names
3. Check credentials file path matches `CLOUDFLARED_CREDENTIALS_DIR`
4. Restart cloudflared:
   ```bash
   docker exec kioku-stateful supervisorctl restart cloudflared
   ```
