---
title: "Authentication"
---
All API endpoints require a JWT bearer token, except `/health`, `/auth/register/*`,
`/auth/signin`, and `/auth/token` (the routes that issue or exchange for a token in the
first place).

```
Authorization: Bearer <token>
```

A user can belong to multiple workspaces. To operate on a workspace other than the
token's default, send `X-Workspace-Id: <workspace_id>` — it must be one of the token's
memberships.

## Register Admin

<Endpoint method="POST" path="/auth/register/admin" />

Creates a new workspace + admin user. Returns an auth session with JWT token.

### Request

```json
{
    "workspace_name": "Acme Corp",
    "workspace_slug": "acme-corp",
    "email": "admin@acme.com",
    "name": "Admin User",
    "password": "securepassword"
}
```

<Note>
  Password must be at least 8 characters. `workspace_slug` is optional — derived from `workspace_name` if omitted.
</Note>

### Response

```json
{
    "user_id": "u-1",
    "email": "admin@acme.com",
    "name": "Admin User",
    "workspace_id": "w-1",
    "workspace_name": "Acme Corp",
    "workspace_slug": "acme-corp",
    "role": "admin",
    "token": "eyJhbGci..."
}
```

## Register Personal

<Endpoint method="POST" path="/auth/register/personal" />

Registers a standalone user with an auto-created, auto-named workspace (`"{name}'s Workspace"`).

## Register Member

<Endpoint method="POST" path="/auth/register/member" />

Registers a team member. Requires a `workspace_slug` and a valid pending invite matching the email. Blocked if the target workspace is on the free tier and already has a member.

## Sign In

<Endpoint method="POST" path="/auth/signin" />

```json
{
    "email": "admin@acme.com",
    "password": "securepassword"
}
```

Returns the same `AuthSession` object as registration, using the user's oldest workspace membership as the active workspace.

## Sign Out

<Endpoint method="POST" path="/auth/signout" />

Invalidates the current token server-side (deletes its `auth_tokens` row) — not just a client-side clear. Requires auth.

## Get Current User

<Endpoint method="GET" path="/auth/me" />

Returns the current user's `AuthSession`.

## API Key Exchange

<Endpoint method="POST" path="/auth/token" />

Exchange a workspace API key for a JWT. Use the `X-API-Key` header:

```
X-API-Key: kioku_abc123...
```

Legacy `cmp_...` keys are still accepted for backward compatibility. This is used by the CLI (`kioku signin --api-key ...`) for long-lived authentication without storing passwords.

## Workspaces

<Endpoint method="GET" path="/workspaces" />

List every workspace the token's memberships include.

<Endpoint method="POST" path="/workspaces" />

Create an additional workspace. Returns a fresh token with the new membership added.

```json
{ "name": "New Team" }
```
