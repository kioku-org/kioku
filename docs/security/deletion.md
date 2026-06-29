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
kioku knowledge-delete <doc-id>

# or via REST
curl -X DELETE http://localhost:9100/knowledge/documents/<id> \
  -H "Authorization: Bearer $TOKEN"
```

Deleting a document removes the metadata from PostgreSQL and all chunk embeddings from Qdrant.

## Delete a User

Via admin API:

```bash
curl -X DELETE http://localhost:8056/admin/users/<user_id> \
  -H "X-Admin-Token: $VEXA_ADMIN_API_TOKEN"
```

This removes the user record. Company data (meetings, documents) is not deleted unless the company is also deleted.

## Delete All Data for a Company

To fully remove a tenant's data, you need to:

1. Delete all meetings (and their embeddings)
2. Delete all documents (and their embeddings)
3. Delete all users in the company
4. Delete the company record

There is no single "delete company" API call yet. For self-hosted deployments, you can delete directly from the database:

```sql
-- Inside the PostgreSQL container
DELETE FROM meetings WHERE company_id = 'c-123';
DELETE FROM documents WHERE company_id = 'c-123';
DELETE FROM users WHERE company_id = 'c-123';
DELETE FROM companies WHERE id = 'c-123';
```

Then clean up orphaned Qdrant vectors:

```bash
# Delete all vectors for a company (Qdrant filter delete)
curl -X POST http://localhost:6333/collections/kioku/points/delete \
  -H "Content-Type: application/json" \
  -d '{"filter": {"must": [{"key": "company_id", "match": {"value": "c-123"}}]}}'
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
