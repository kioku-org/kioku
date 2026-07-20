---
title: "Production Checklist"
description: "Steps to harden a Kioku deployment before going to production."
---

## Secrets

- [ ] `HIVEMIND_JWT_SECRET` — generate with `openssl rand -hex 32`
- [ ] `HIVEMIND_ENCRYPTION_SECRET` — generate with `openssl rand -hex 32`
- [ ] `VEXA_ADMIN_API_TOKEN` — generate with `openssl rand -hex 20`
- [ ] `NEXTAUTH_SECRET` — generate with `openssl rand -base64 32`
- [ ] `DB_PASSWORD` — non-default, strong password
- [ ] `REDIS_PASSWORD` — required if Redis port is exposed to the internet
- [ ] `MINIO_ACCESS_KEY` / `MINIO_SECRET_KEY` — change from defaults

## Networking

- [ ] Only expose ports 9100, 8056, 3001, 18888 (or none if using Cloudflare Tunnel)
- [ ] PostgreSQL (5432), Qdrant (6334 HTTP / 6335 gRPC), MinIO (9000), Ollama (11434) stay internal
- [ ] If using RunPod overflow: expose Redis (6379) and meeting-api (8080) with password auth
- [ ] Set `CORS_ORIGINS` to your actual domain instead of `*`
- [ ] Set `NEXTAUTH_URL` and `VEXA_PUBLIC_URL` to your actual public URLs

## TLS

- [ ] All public endpoints served over HTTPS
- [ ] Cloudflare Tunnel handles TLS termination if using that approach
- [ ] Or: put a reverse proxy (nginx/caddy) in front with Let's Encrypt certs

## Dashboard Access

- [ ] `VEXA_ALLOW_DIRECT_LOGIN=true` is fine for personal use; disable for teams and use OAuth
- [ ] Set `GOOGLE_CLIENT_ID` / `GOOGLE_CLIENT_SECRET` for Google sign-in
- [ ] Or configure SMTP for email magic links

## Storage

- [ ] Docker volumes are on a disk with adequate space (50 GB minimum)
- [ ] Set up regular backups for `kioku-postgres-data` and `kioku-qdrant-data`
- [ ] If `RECORDING_ENABLED=true`, ensure MinIO bucket has enough space

## GPU

- [ ] `nvidia-container-toolkit` installed and verified (`docker run --gpus all nvidia/cuda:12.0-base nvidia-smi`)
- [ ] Local whisper model pre-warmed before first meeting (start a test bot) — skip if using `STT_BACKEND=chirp`/`gpt4o` cloud transcription
- [ ] `BOT_WHISPER_MODEL` sized to available VRAM (see [GPU vs CPU Modes](/deployment/gpu-cpu-modes))

## Monitoring

```bash
# Quick health check
curl https://your-domain.com/health             # Hivemind
curl https://meetings.your-domain.com/          # Vexa gateway
curl https://mcp.your-domain.com/health         # MCP

# Inside container
docker exec kioku-stateful supervisorctl status
```

Check that all processes show `RUNNING`. Any `FATAL` or `EXITED` status needs investigation.

## Backup Commands

```bash
# PostgreSQL
docker exec kioku-stateful pg_dump -U kioku kioku > backup.sql

# Qdrant (snapshot)
curl -X POST http://localhost:6334/collections/knowledge/snapshots

# Full volume backup (stop container first or use consistent snapshot)
docker run --rm -v kioku-postgres-data:/data -v $(pwd):/backup \
  busybox tar czf /backup/postgres-$(date +%Y%m%d).tar.gz /data
```
