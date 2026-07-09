# Vexa ↔ Hivemind Credential Linking

Hivemind and Vexa are separate services with separate credential systems —
different schemas, different hashing, no shared users table. There is no
single merged "users" table; the two are linked by reference, not by schema
merge. This documents the resulting shape after the per-user Vexa token
link, with Hivemind's `company` → `workspace` rename applied.

## Schema

```mermaid
erDiagram
    "hivemind.workspaces" ||--o{ "hivemind.users" : "workspace_members"
    "hivemind.workspaces" ||--o{ "hivemind.workspace_api_keys" : owns
    "hivemind.users" ||--o{ "hivemind.workspace_api_keys" : owns
    "hivemind.users" ||--o| "vexa.users" : "linked by email (vexa_user_id)"
    "hivemind.users" ||--o| "vexa.api_tokens" : "linked by email (vexa_token_id)"
    "vexa.users" ||--o{ "vexa.api_tokens" : owns

    "hivemind.workspaces" {
        uuid id PK
        varchar name
        varchar slug UK
        varchar tier "free | pro | teams, default free"
    }

    "hivemind.users" {
        uuid id PK
        varchar email UK
        varchar name
        text password_hash
        integer vexa_user_id "logical FK -> vexa.users.id"
        integer vexa_token_id "logical FK -> vexa.api_tokens.id"
        text vexa_token "plaintext, mirrors vexa.api_tokens.token"
    }

    "hivemind.workspace_api_keys" {
        uuid id PK
        uuid workspace_id FK
        uuid user_id FK
        varchar name
        varchar key_prefix "kioku_ prefix (legacy: cmp_)"
        text key_hash "bcrypt"
    }

    "vexa.users" {
        integer id PK
        varchar email UK
        varchar name
        integer max_concurrent_bots "default 1 (free tier)"
        varchar tier "free | pro | team, default free"
        jsonb data
    }

    "vexa.api_tokens" {
        integer id PK
        varchar token UK "plaintext"
        integer user_id FK
        text_array scopes
        varchar name
    }
```

A single Hivemind `user` can belong to and switch between **multiple workspaces**
(`memberships` in the JWT) — the diagram above shows one user's membership edge into
one workspace, but the relationship is many-to-many via `workspace_members`.

## Why two systems, not one

- **Hivemind** (`services/hivemind`, schema `hivemind`): `kioku_<hex>` keys,
  bcrypt-hashed in `workspace_api_keys`. Owns CLI/MCP-facing auth — the
  credential a user actually holds (`kioku signin --api-key ...`).
- **Vexa** (`services/admin-api` + `services/meeting-api`, schema `vexa`):
  plaintext `api_tokens`, validated by `admin-api`'s `POST /internal/validate`.
  Owns bot-management auth (`X-API-Key` on the gateway's `/bots*` routes).

These predate each other and were never designed to share a credential
format (bcrypt hash vs. plaintext lookup can't be reconciled into one
literal string safely). Rather than force one raw secret to double as both,
each Hivemind user gets **their own linked Vexa user + Vexa token**,
provisioned lazily.

## Provisioning flow

1. A Hivemind user calls anything that proxies to Vexa (`POST /vexa/bots`,
   `GET /vexa/bots/status`, `DELETE /vexa/bots/:platform/:native_meeting_id`,
   `GET /vexa/meetings`, or the `GET /vexa/token` exchange endpoint).
2. `resolve_vexa_api_key` (`services/hivemind/src/handlers/vexa.rs`) checks
   `hivemind.users.vexa_token`. If already set, use it — no network call.
3. If unset, find-or-create the Vexa user by email
   (`POST {vexa_admin_api_url}/admin/users`, admin-api's existing
   find-or-create-by-email endpoint) and mint a token for them
   (`POST /admin/users/{id}/tokens`), both authenticated with the shared
   `vexa_admin_token` (an actual Vexa admin secret, not a per-user one).
4. Store the result on `hivemind.users` (`vexa_user_id`, `vexa_token_id`,
   `vexa_token`) so every future call for that user is a local read, not a
   re-provision.
5. If provisioning fails or isn't configured (`vexa_admin_token` empty),
   fall back to the shared `vexa_admin_token` directly — preserves the
   original single-shared-credential behavior rather than breaking outright.

This replaces "every Hivemind user's Vexa calls authenticate as one static
shared service account" with "every Hivemind user has their own Vexa
identity and token," without requiring a database/schema merge or any
downtime-risking migration of either service's existing credential table.

## MCP unification

The single unified MCP server (`services/mcp`, `kioku-mcp`, port 18888) hosts both the
knowledge tools and the meeting/bot tools behind one endpoint. For meeting/bot tools, it
calls `GET /vexa/token` on Hivemind first (`resolve_vexa_key` in `handler.rs`) to resolve
whichever Kioku credential it was given (JWT, `kioku_...` key, or a raw Vexa key) into the
caller's per-user Vexa API key, then forwards to the Vexa gateway as `X-API-Key`. For
knowledge tools, it forwards the caller's original bearer token straight to Hivemind, which
validates it itself. So `kioku mcp`'s single printed credential works against every tool —
knowledge and meetings alike. See [MCP overview](/mcp/overview) for the full tool list.
