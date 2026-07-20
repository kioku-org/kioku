---
title: "Deletion"
description: "How to delete meetings, documents, and user data from Kioku."
---

## Delete a Meeting

There is no per-meeting delete endpoint yet — Hivemind's `/meetings` routes are
list/create/get/transcript only, no `DELETE`. To remove a single meeting's data today,
delete directly from PostgreSQL (cascades to `knowledge_chunks` via `ON DELETE CASCADE`)
and clean up its Qdrant vectors:

```sql
-- Inside the PostgreSQL container
DELETE FROM meetings WHERE id = '<meeting-id>';
```

```bash
# Qdrant filter delete for that meeting's vectors (metadata.meeting_id, see Qdrant Collections above)
curl -X POST http://localhost:6334/collections/knowledge/points/delete \
  -H "Content-Type: application/json" \
  -d '{"filter": {"must": [{"key": "metadata.meeting_id", "match": {"value": "<meeting-id>"}}]}}'
```

If `RECORDING_ENABLED=true`, also remove the recording objects for that meeting from the
MinIO `vexa-recordings` bucket.

## Delete a Document

```bash
kioku docs --delete <doc-id>

# or via REST
curl -X DELETE http://localhost:9100/knowledge/documents/<id> \
  -H "Authorization: Bearer $TOKEN"
```

Deleting a document removes the metadata from PostgreSQL and all chunk embeddings from Qdrant.

## Delete a User

There is no user-delete endpoint on either side today. `admin-api` (port 8001, not 8056)
only exposes `PATCH /admin/users/{user_id}` (update fields) and `DELETE
/admin/tokens/{token_id}` (revoke a single API token) — no way to remove the Vexa-side user
record itself via the API. Hivemind has no user-delete route either.

To fully remove a user, delete their row directly from both databases: the Vexa-side
`users` table (via `admin-api`'s DB) and Hivemind's `hivemind.users` table (cascades to
their `workspace_members` rows). See [Vexa ↔ Hivemind credential
linking](/architecture/vexa-hivemind-credentials) for how the two are linked.

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
curl -X POST http://localhost:6334/collections/knowledge/points/delete \
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
