---
title: "Authentication"
---
All API endpoints except `/health` and `/auth/register/*` require a JWT bearer token.

```
Authorization: Bearer <token>
```

## Register Admin

<Endpoint method="POST" path="/auth/register/admin" />

Creates a new company + admin user. Returns an auth session with JWT token.

### Request

```json
{
    "company_name": "Acme Corp",
    "email": "admin@acme.com",
    "name": "Admin User",
    "password": "securepassword"
}
```

<Note>
  Password must be at least 8 characters.
</Note>

### Response

```json
{
    "user_id": "u-1",
    "email": "admin@acme.com",
    "name": "Admin User",
    "company_id": "c-1",
    "company_name": "Acme Corp",
    "company_slug": "acme-corp",
    "role": "admin",
    "token": "eyJhbGci..."
}
```

## Register Personal

<Endpoint method="POST" path="/auth/register/personal" />

Register a standalone account (no company).

## Register Member

<Endpoint method="POST" path="/auth/register/member" />

Register a team member. Requires a valid invite.

## Sign In

<Endpoint method="POST" path="/auth/signin" />

```json
{
    "email": "admin@acme.com",
    "password": "securepassword"
}
```

Returns the same `AuthSession` object as registration.

## Sign Out

<Endpoint method="POST" path="/auth/signout" />

Invalidates the current token. Requires auth.

## Get Current User

<Endpoint method="GET" path="/auth/me" />

Returns the current user's `AuthSession`.

## API Key Exchange

<Endpoint method="POST" path="/auth/token" />

Exchange a company API key for a JWT. Use the `X-API-Key` header:

```
X-API-Key: koku_abc123...
```

This is used by the CLI for long-lived authentication without storing passwords.