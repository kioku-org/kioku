---
title: "Storage"
description: "What data Kioku stores and where."
---

## Storage Systems

Kioku uses four storage backends, all running locally inside (or alongside) the stateful container:

| System | Volume | What's stored |
|---|---|---|
| **PostgreSQL** | `kioku-postgres-data` | Users, companies, meetings, documents, sessions, messages, API keys |
| **Qdrant** | `kioku-qdrant-data` | Vector embeddings for all meetings and documents |
| **MinIO** | `kioku-minio-data` | Meeting audio recordings (only if `RECORDING_ENABLED=true`) |
| **Redis** | `kioku-redis-data` | Live transcription streams, bot scheduling (ephemeral) |

## PostgreSQL Schema (Key Tables)

- `companies` — tenant records
- `users` — user accounts (email, hashed password, role, `company_id`)
- `meetings` — meeting metadata and full transcript JSON
- `documents` — document metadata (PDF text not stored, only embeddings in Qdrant)
- `sessions` — AI chat sessions
- `messages` — session messages and agent traces
- `api_keys` — long-lived API key records

Passwords are hashed (bcrypt). Sensitive fields use application-level encryption (`HIVEMIND_ENCRYPTION_SECRET`).

## Qdrant Collections

All embeddings are in a single collection. Each vector stores:
- `company_id` — for tenant isolation filtering
- `chunk_type` — `transcript` or `document`
- `text` — the original chunk text
- Source metadata (meeting ID + speaker, or document ID + chunk index)

## Recording Storage

Recordings are off by default (`RECORDING_ENABLED=false`). When enabled:
- Audio is captured by the bot and streamed to MinIO during the meeting
- Stored as `.webm` or `.mp4` in the `vexa-recordings` bucket
- Accessible via the Vexa API

## Backup

None of Kioku's storage backends include built-in backup scheduling. On self-hosted deployments:

```bash
# PostgreSQL logical dump
docker exec kioku-stateful pg_dump -U $DB_USER $DB_NAME > meetings-backup.sql

# Qdrant snapshot (live, non-destructive)
curl -X POST http://localhost:6333/collections/kioku/snapshots

# Volume-level backup (best done while container is stopped)
docker run --rm \
  -v kioku-postgres-data:/data \
  -v $(pwd):/out \
  busybox tar czf /out/postgres.tar.gz /data
```

## Volume Locations

Named Docker volumes are managed by the Docker engine. To find the host path:

```bash
docker volume inspect kioku-postgres-data
# → "Mountpoint": "/var/lib/docker/volumes/kioku-postgres-data/_data"
```

Volumes are not encrypted at rest by default. Use OS-level disk encryption (LUKS on Linux) or encrypted volume mounts to protect data at rest.
