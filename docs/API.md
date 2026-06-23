# Hivemind API Reference

Base URL: `http://localhost:9100` (local) or `https://api.kioku.chat` (production)

## Authentication

All endpoints except `/health` and `/auth/register/*` require a JWT bearer token:

```
Authorization: Bearer <token>
```

Tokens are obtained via `POST /auth/signin` or `POST /auth/register/*`.

---

## Health

### `GET /health`

Returns `200` with `{"status":"ok"}`.

---

## Auth

### `POST /auth/register/admin`

Register a new company admin. Creates company + user.

| Body | Type | Required |
|---|---|---|
| `company_name` | string | yes |
| `email` | string | yes |
| `name` | string | yes |
| `password` | string | yes (min 8 chars) |

Returns: `AuthSession` (token, user info, company info).

### `POST /auth/register/personal`

Register a personal account (no company).

### `POST /auth/register/member`

Register a team member (requires invite).

### `POST /auth/signin`

| Body | Type |
|---|---|
| `email` | string |
| `password` | string |

Returns: `AuthSession`.

### `POST /auth/signout`

Invalidates the current token. Requires auth.

### `GET /auth/me`

Returns: `AuthSession` (current user info).

### `POST /auth/token`

Exchange a company API key for a JWT. Use `X-API-Key` header instead of body.

---

## Company

### `GET /company/config`

Returns company configuration.

### `PUT /company/config`

Update company configuration.

---

## Members

### `GET /company/members`

List all company members.

### `DELETE /company/members/:user_id`

Remove a member from the company.

### `PUT /company/members/:user_id/:role`

Change a member's role.

---

## Invites

### `GET /company/invites`

List pending invitations.

### `POST /company/invites`

Create an invitation. Returns invite link.

### `DELETE /company/invites/:invite_id`

Revoke an invitation.

---

## Provider API Keys

Encrypted storage for third-party API keys (OpenAI, Anthropic, etc.).

### `GET /company/apikeys/:user_id`

List API keys for a user.

### `POST /company/apikeys`

| Body | Type |
|---|---|
| `provider` | string |
| `plain_key` | string |

Stores the key encrypted (AES-GCM).

### `DELETE /company/apikeys/key/:key_id`

Delete an API key.

---

## Company Auth Keys (CLI)

Long-lived API keys for CLI authentication.

### `GET /company/auth-keys`

List auth keys.

### `POST /company/auth-keys`

| Body | Type |
|---|---|
| `name` | string |

Returns: key prefix + full key (shown once).

### `DELETE /company/auth-keys/:key_id`

Delete an auth key.

---

## Meetings

### `GET /meetings`

List all meetings for the company.

### `POST /meetings`

Ingest a meeting transcript. Chunks are embedded and stored in Qdrant.

| Body | Type |
|---|---|
| `title` | string |
| `date` | int (epoch ms) |
| `duration_seconds` | int |
| `participants` | string[] |
| `transcript` | TranscriptSegment[] |

`TranscriptSegment`:
| Field | Type |
|---|---|
| `speaker` | string |
| `text` | string |
| `start_time` | int |
| `end_time` | int |

---

## Knowledge

### `POST /knowledge/search`

Vector similarity search across all knowledge (documents + meetings).

| Body | Type |
|---|---|
| `query` | string |
| `limit` | int (default 5) |

Returns: `KnowledgeSearchResult[]` with `id`, `score`, `text`.

### `GET /knowledge/documents`

List uploaded documents.

### `POST /knowledge/documents`

Upload a PDF. Multipart form with `file` field. Text is extracted, chunked, embedded, and stored.

### `DELETE /knowledge/documents/:document_id`

Delete a document and its embeddings.

---

## Sessions

### `GET /sessions`

List all sessions.

### `POST /sessions`

| Body | Type |
|---|---|
| `title` | string |
| `mode` | string (e.g. "research") |

### `GET /sessions/:session_id`

Get session details.

### `PATCH /sessions/:session_id`

Update session (e.g. title).

### `DELETE /sessions/:session_id`

Delete a session.

---

## Messages

### `GET /sessions/:session_id/messages`

List messages in a session.

### `POST /sessions/:session_id/messages`

| Body | Type |
|---|---|
| `id` | string (UUID) |
| `role` | string ("user"/"assistant") |
| `content` | ContentPart[] |
| `timestamp` | int (epoch ms) |

---

## Traces

### `GET /sessions/:session_id/traces`

List traces for a session.

### `POST /sessions/:session_id/traces`

Create a trace (execution record).

### `PATCH /sessions/:session_id/traces/:trace_id`

Update a trace.

---

## Usage

### `POST /usage`

Record token usage.

### `GET /usage/summary`

Get usage summary per user.

---

## Vexa Proxy

### `POST /vexa/bots`

Request a meeting bot. Proxied to Vexa meeting-api.

### `GET /vexa/meetings`

List Vexa meetings.

---

## MCP

The MCP endpoint is available at `/mcp` with streamable-http transport.

### Tools

| Tool | Description |
|---|---|
| `kioku_search` | Search the knowledge base |
| `kioku_list_meetings` | List all meetings |
| `kioku_get_transcript` | Get a meeting transcript |
| `kioku_get_meeting` | Get meeting details |
| `kioku_list_documents` | List uploaded documents |
| `kioku_delete_document` | Delete a document |
| `kioku_ingest_meeting` | Ingest a meeting transcript |

Authentication via `Authorization: Bearer <token>` header. Company/user context extracted from MCP session `_meta`.