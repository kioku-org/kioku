---
title: "Hivemind"
---
Hivemind is Kioku's core API server, built in Rust with axum. Binary name: `kioku-hivemind`. Listens on port **9100**.

## Responsibilities

- **Authentication** — admin/personal/member registration, JWT sessions, API key exchange
- **Workspaces** — multi-workspace membership per user, invites, roles, long-lived API keys
- **Knowledge** — document upload (PDF/DOCX/PPTX/TXT/MD), text extraction, chunking, embedding via Ollama, vector search via Qdrant
- **Sessions** — conversation sessions with messages and trace steps
- **Meetings** — meeting ingest (transcript → embeddings → searchable knowledge), coding-session ingest
- **Vexa proxy** — bot spawn/stop/status requests, meeting listing, per-user Vexa credential provisioning
- **Vexa/Hivemind credential exchange** — `GET /vexa/token`, used by `services/mcp` to trade a Hivemind credential for the caller's own Vexa API key

<Note>
  Hivemind does **not** run its own MCP server. That capability was fully migrated into the standalone `services/mcp` binary (`kioku-mcp`), which hosts every MCP tool — knowledge and meeting/bot alike — behind one endpoint. See [MCP overview](/mcp/overview).
</Note>

## Stack

| Technology | Purpose |
|---|---|
| Rust + axum 0.7 | HTTP server |
| sqlx 0.8 + Postgres | Relational data + migrations (schema `hivemind`) |
| Qdrant (`qdrant-client`) | Vector similarity search, single collection `knowledge` |
| Ollama (`ollama-rs`) | Local embeddings, default model `nomic-embed-text-v2-moe` |
| jsonwebtoken | JWT auth tokens (HS256) |
| bcrypt | Password hashing + API key hashing |
| `pdf-extract`, `docx-rs`, custom zip/XML parser | Document text extraction (PDF, DOCX, PPTX) |

## Source

```
services/hivemind/
├── src/
│   ├── main.rs               # entry: init embedder + Qdrant, build router, serve
│   ├── router.rs              # all route definitions
│   ├── config.rs              # env-driven Settings (see below)
│   ├── handlers/              # HTTP handlers (auth, workspace, knowledge, meeting, vexa, session...)
│   ├── repos/                 # data access layer (repos/auth.rs holds credential resolution)
│   ├── services/              # business logic (knowledge chunking, pdf/docx/pptx/ocr, vector store)
│   └── middleware.rs           # AuthContext extractor (JWT + active-workspace resolution)
├── migrations/                # SQL migrations, see below
└── tests/                     # integration tests
```

## Workspaces (formerly "companies")

Hivemind's tenancy model was renamed end-to-end from **company** to **workspace** (migration `009_rename_company_to_workspace.sql`), and the model changed from one-company-per-user to **multi-workspace membership**: a single user can belong to and switch between several workspaces.

- Registration types: `admin` (creates a new workspace), `personal` (auto-named `"{name}'s Workspace"`, standalone), `member` (joins an existing workspace via a pending invite — blocked if the target workspace is on the free tier and already has a member).
- JWT claims (`Claims`): `user_id`, `workspace_id` (the token's default/active workspace), `role`, `memberships: [{workspace_id, role}]`, `exp`.
- **Active workspace selection**: every request's `AuthContext` extractor resolves the operating workspace from an optional `X-Workspace-Id` header (must be one of the token's memberships) or falls back to the token's default workspace. Sign-out invalidates the token server-side (checked against the `auth_tokens` table on every request), not just client-side.
- Free tier is capped at 1 member per workspace (enforced both at invite-creation time and again at member-registration time, in case tier changed in between).

See [Multi-Tenancy](/core-concepts/multi-tenancy) for the tenant-isolation model and [Kioku CLI](/architecture/cli) for the `kioku ws`/`kioku invite` commands that drive this.

## Every HTTP route

| Method | Path | Auth | Purpose |
|---|---|---|---|
| GET | `/health` | none | Liveness check |
| POST | `/internal/provision` | `X-Internal-Secret` (falls back to JWT secret if unset) | Dashboard→Hivemind: find-or-create user by email, issue a JWT session |
| POST | `/auth/register/admin` | none | Create workspace + admin user |
| POST | `/auth/register/personal` | none | Create a standalone user + auto-named workspace |
| POST | `/auth/register/member` | none | Join an existing workspace via a pending invite |
| POST | `/auth/signin` | none | Email + password → `AuthSession` |
| POST | `/auth/signout` | JWT | Deletes all `auth_tokens` rows for the user+workspace (server-side revocation) |
| GET | `/auth/me` | JWT | Current `AuthSession` |
| POST | `/auth/token` | `X-API-Key` | Exchange a `kioku_...` (or legacy `cmp_...`) key for a JWT session |
| GET | `/workspace/config` | JWT | Get active workspace's config |
| PUT | `/workspace/config` | JWT admin | Update active workspace's config |
| GET | `/workspace/members` | JWT | List members of the active workspace |
| DELETE | `/workspace/members/:user_id` | JWT admin | Remove a member |
| PUT | `/workspace/members/:user_id/:role` | JWT admin | Change a member's role |
| GET | `/workspace/invites` | JWT | List invites for the active workspace |
| POST | `/workspace/invites` | JWT admin | Create an invite (blocked on free tier) |
| DELETE | `/workspace/invites/:invite_id` | JWT admin | Revoke an invite |
| GET | `/workspaces` | JWT | List every workspace the token's memberships include |
| POST | `/workspaces` | JWT | Create an additional workspace; returns a token with the new membership added |
| POST | `/workspaces/:workspace_id_or_slug/invites` | JWT admin-of-that-workspace | Invite into a specific (non-active) workspace by id or slug |
| POST | `/workspaces/:workspace_id_or_slug/join` | JWT | Accept a pending invite as an already-registered user (by workspace id or slug); returns a fresh token with the new membership added |
| GET | `/workspace/auth-keys` | JWT | List long-lived API keys |
| POST | `/workspace/auth-keys` | JWT admin | Create a `kioku_...` API key |
| DELETE | `/workspace/auth-keys/:key_id` | JWT admin | Delete an API key |
| GET | `/meetings` | JWT | List meetings |
| POST | `/meetings` | JWT | Ingest a meeting transcript (embeds asynchronously in the background) |
| GET | `/meetings/:meeting_id` | JWT | Meeting detail |
| GET | `/meetings/:meeting_id/transcript` | JWT | Transcript chunks |
| POST | `/knowledge/search` | JWT | Vector search across documents + meetings + sessions |
| GET | `/knowledge/documents` | JWT | List documents |
| POST | `/knowledge/documents` | JWT | Upload a document (multipart, 50MB cap; PDF/DOCX/PPTX/TXT/MD) |
| DELETE | `/knowledge/documents/:document_id` | JWT | Delete a document + its Qdrant vectors |
| POST | `/knowledge/sessions` | JWT | Ingest arbitrary content (e.g. a coding session) via paragraph-aware chunking |
| GET | `/sessions` | JWT | List chat sessions |
| POST | `/sessions` | JWT | Create a chat session |
| GET | `/sessions/:session_id` | JWT | Session detail |
| PATCH | `/sessions/:session_id` | JWT | Update a session |
| DELETE | `/sessions/:session_id` | JWT | Delete a session |
| GET | `/sessions/:session_id/messages` | JWT | List messages |
| POST | `/sessions/:session_id/messages` | JWT | Create a message |
| GET | `/sessions/:session_id/traces` | JWT | List trace steps |
| POST | `/sessions/:session_id/traces` | JWT | Create a trace step |
| PATCH | `/sessions/:session_id/traces/:trace_id` | JWT | Update a trace step |
| POST | `/vexa/bots` | JWT | Proxy to Vexa `POST /bots` (auto-fills `bot_name` from the workspace name if omitted) |
| GET | `/vexa/bots/status` | JWT | Proxy to Vexa `GET /bots/status` |
| DELETE | `/vexa/bots/:platform/:native_meeting_id` | JWT | Proxy to Vexa `DELETE /bots/{platform}/{id}` |
| GET | `/vexa/meetings` | JWT | Proxy to Vexa `GET /meetings` |
| GET | `/vexa/token` | JWT | Resolve and return the caller's per-user Vexa API key |

## Auth details

- Passwords: bcrypt, minimum 8 characters.
- API key format: `kioku_<32-hex>`, looked up by a 14-char prefix. Legacy `cmp_<hex>` keys (12-char prefix) still validate for backward compatibility. Stored bcrypt-hashed in `workspace_api_keys`.
- `resolve_claims_from_token` (the shared credential resolver behind `/vexa/token`) accepts a JWT, a `kioku_`/`cmp_` API key, or a raw session token — this is what lets `services/mcp` forward whatever credential type a caller was already holding.

## Knowledge pipeline

Supported upload formats: **PDF, DOCX, PPTX, TXT, MD** (50MB cap), with a zip-bomb guard on DOCX/PPTX (rejects if the uncompressed size in the zip central directory exceeds 300MB, without decompressing).

- PDF → `pdf-extract`; falls back to OCR if extracted text looks too thin (scanned/image-only PDF).
- DOCX → `docx-rs`.
- PPTX → a custom zip/XML parser.
- TXT/MD → read as raw UTF-8.
- Chunking: word-window splitter (400 words, 80-word overlap) for documents and meeting transcript segments; a separate **paragraph-aware splitter** (splits on blank lines first, word-windows only oversized paragraphs, carries the last paragraph forward as overlap context) for `/knowledge/sessions` ingest.
- Embedding: Ollama HTTP API, default model `nomic-embed-text-v2-moe`.
- Vector store: Qdrant, single collection `knowledge`. Every point's payload includes `workspace_id`; every read/write filters on it — cross-workspace search is structurally impossible at the query level. `/knowledge/search` short-circuits to `[]` (no error) if the workspace has zero indexed chunks.

## Meetings and coding sessions

- `POST /meetings` writes the meeting row synchronously, then embeds the transcript in a background task — the HTTP response returns before embedding finishes.
- `coding_sessions` table stores ingested sessions (title, summary, decisions, tags, date); `knowledge_chunks.session_id` links chunks back to their session.
- `POST /knowledge/sessions` is the live ingest path: inserts a `coding_sessions` row, paragraph-chunks the content, inserts `knowledge_chunks` rows tagged `chunk_type=session`, and embeds into Qdrant.

## Vexa credential provisioning

`resolve_vexa_api_key` gives every Hivemind user their own Vexa identity, lazily:

1. If `users.vexa_token` is already set, use it — no network call.
2. Otherwise, find-or-create a Vexa user by email and mint a token for them via admin-api, authenticated with the shared `vexa_admin_token`.
3. Persist the result on the `users` row so future calls are a local read.
4. If provisioning fails or `vexa_admin_token` isn't configured, silently fall back to the shared `vexa_admin_token` rather than erroring.

See [Vexa ↔ Hivemind credential linking](/architecture/vexa-hivemind-credentials) for the full picture, including how `services/mcp` uses `GET /vexa/token` to make one credential work against both the knowledge and meeting toolsets.

## Configuration

All settings are read from env vars (no prefix; `HIVEMIND__JWT_SECRET`-style double-underscore nesting is also supported).

| Setting | Env var | Default |
|---|---|---|
| Postgres host/port/db/user/password | `DB_HOST`/`DB_PORT`/`DB_NAME`/`DB_USER`/`DB_PASSWORD` | `supabase-db` / `5432` / `postgres` / `postgres` / `postgres` |
| Max DB connections | `DB_MAX_CONNECTIONS` | `10` |
| Schema | `DB_SCHEMA` | `hivemind` |
| JWT signing secret | `JWT_SECRET` | `hivemind-secret-change-me` (⚠ must be overridden in production) |
| JWT lifetime | `JWT_TTL_SECONDS` | `2592000` (30 days) |
| Internal secret | `INTERNAL_SECRET` | falls back to `JWT_SECRET` if unset |
| Vexa API URL | `VEXA_API_URL` | `http://vexa-api-gateway:8000` |
| Vexa admin API URL | `VEXA_ADMIN_API_URL` | `http://vexa-admin-api:8001` |
| Vexa admin token | `VEXA_ADMIN_TOKEN` | empty |
| Bind host/port | `HOST`/`PORT` | `0.0.0.0` / `9100` |
| Embedding API URL | `EMBEDDING_API_URL` | `http://localhost:11434` |
| Embedding model | `EMBEDDING_MODEL` | `nomic-embed-text-v2-moe` |
| Qdrant URL | `QDRANT_URL` | Code default `http://localhost:6334`; deployment (`entrypoint-stateful-runtime.sh`) overrides to `http://localhost:6335` — Qdrant's actual gRPC port. `6334` is Qdrant's HTTP/REST port, not gRPC. |
| Qdrant API key | `QDRANT_API_KEY` | empty |

## Migrations

| # | File | Summary |
|---|---|---|
| 001 | `001_init.sql` | Base schema: companies, users, members, invites, meetings, sessions, messages, auth_tokens, knowledge_chunks, company_config |
| 002 | `002_auth_tokens_token_text.sql` | Widen `auth_tokens.token` to `TEXT` |
| 003 | `003_knowledge_documents.sql` | Add `knowledge_documents`; allow `knowledge_chunks.meeting_id` to be NULL |
| 004 | `004_company_api_keys.sql` | Add `company_api_keys` (long-lived CLI keys) |
| 005 | `005_coding_sessions.sql` | Add `coding_sessions`; add `knowledge_chunks.session_id` |
| 006 | `006_vexa_link.sql` | Add `users.vexa_user_id`/`vexa_token_id`/`vexa_token` |
| 007 | `007_kioku_key_prefix.sql` | Widen `key_prefix` for `kioku_` keys (vs. legacy `cmp_`) |
| 008 | `008_company_tier.sql` | Add `companies.tier` (`free`/`pro`/`teams`, default `free`) |
| 009 | `009_rename_company_to_workspace.sql` | Full rename: `companies`→`workspaces`, `company_members`→`workspace_members`, `company_invites`→`workspace_invites`, `company_config`→`workspace_config`, `company_api_keys`→`workspace_api_keys`, and every `company_id` column → `workspace_id` |
| 010 | `010_meetings_vexa_unique.sql` | Dedupe meetings double-ingested by concurrent `run_all_tasks` trigger sites; add a unique index on `(workspace_id, vexa_meeting_id)` so meeting ingestion is idempotent at the DB level |
| 011 | `011_revoke_bot_only_vexa_tokens.sql` | Clear cached per-user Vexa tokens minted with `scope=bot` only (pre-dated the `tx` scope requirement for transcript/meeting reads) — forces re-provisioning with `scopes=bot,tx` on next use |
