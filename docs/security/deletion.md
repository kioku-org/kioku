---
title: "Deletion"
description: "How to delete meetings, documents, and user data from Kioku."
---

## Delete a Meeting

```bash
# CLI (coming soon)
# kioku meetings-delete <id>

# REST (via Hivemind API — requires admin or owner)
curl -X DELETE http://localhost:9100/meetings/<id> \
  -H "Authorization: Bearer $TOKEN"
```

Deleting a meeting removes:
- The meeting record and transcript from PostgreSQL
- All vector embeddings for that meeting from Qdrant
- The recording from MinIO (if `RECORDING_ENABLED=true`)

## Delete a Document

```bash
kioku docs --delete <doc-id>

# or via REST
curl -X DELETE http://localhost:9100/knowledge/documents/<id> \
  -H "Authorization: Bearer $TOKEN"
```

Deleting a document removes the metadata from PostgreSQL and all chunk embeddings from Qdrant.

## Delete a User

Via the Vexa admin API (note: `admin-api` listens on port 8001, not 8056):

```bash
curl -X DELETE http://localhost:8001/admin/users/<user_id> \
  -H "X-Admin-API-Key: $VEXA_ADMIN_API_TOKEN"
```

This removes the Vexa-side user record. It does not touch the linked Hivemind user or
workspace data (meetings, documents) — those are separate systems, see
[Vexa ↔ Hivemind credential linking](/architecture/vexa-hivemind-credentials).

## Delete All Data for a Workspace

To fully remove a tenant's data, you need to:

1. Delete all meetings (and their embeddings)
2. Delete all documents (and their embeddings)
3. Remove all members from the workspace
4. Delete the workspace record

There is no single "delete workspace" API call yet. For self-hosted deployments, you can delete directly from the database (schema `hivemind`):

```sql
-- Inside the PostgreSQL container
DELETE FROM knowledge_chunks WHERE workspace_id = 'w-123';
DELETE FROM meetings WHERE workspace_id = 'w-123';
DELETE FROM knowledge_documents WHERE workspace_id = 'w-123';
DELETE FROM workspace_members WHERE workspace_id = 'w-123';
DELETE FROM workspaces WHERE id = 'w-123';
```

Then clean up orphaned Qdrant vectors:

```bash
# Delete all vectors for a workspace (Qdrant filter delete)
curl -X POST http://localhost:6333/collections/knowledge/points/delete \
  -H "Content-Type: application/json" \
  -d '{"filter": {"must": [{"key": "workspace_id", "match": {"value": "w-123"}}]}}'
```

## Bot Session Cookies

Stored cookies for authenticated bot mode are in the `kioku-cookie-data` volume:

```bash
# Delete cookies for a specific user
curl -X DELETE http://localhost:8099/userdata/<user_id> \
  -H "Authorization: Bearer $COOKIE_SERVICE_TOKEN"
```

## Full Wipe

To destroy all Kioku data on a self-hosted instance:

```bash
docker compose -f docker-compose.stateful.yml down -v
```

The `-v` flag removes all named volumes. This is irreversible.
