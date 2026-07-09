---
title: "Workspace API"
---
Manage workspace config, members, invites, and API keys. All routes operate on the
caller's active workspace (or the one named by `X-Workspace-Id`) — see [Authentication](/api/authentication).

## Workspace Config

### Get Config
<Endpoint method="GET" path="/workspace/config" />

### Update Config
<Endpoint method="PUT" path="/workspace/config" />

```json
{
    "hivemind_enabled": true
}
```

Requires the `admin` role.

## Workspaces (multi-workspace)

### List My Workspaces
<Endpoint method="GET" path="/workspaces" />

Returns every workspace the caller's token has a membership in.

### Create Workspace
<Endpoint method="POST" path="/workspaces" />

```json
{ "name": "New Team" }
```

Creates a workspace and returns a new token that includes the added membership.

### Invite Into a Specific Workspace
<Endpoint method="POST" path="/workspaces/:workspace_id_or_slug/invites" />

Requires the `admin` role in the target workspace (not necessarily the caller's active one).

## Members

### List Members
<Endpoint method="GET" path="/workspace/members" />

### Remove Member
<Endpoint method="DELETE" path="/workspace/members/:user_id" />

### Update Member Role
<Endpoint method="PUT" path="/workspace/members/:user_id/:role" />

## Invites

### List Invites
<Endpoint method="GET" path="/workspace/invites" />

### Create Invite
<Endpoint method="POST" path="/workspace/invites" />

```json
{ "email": "teammate@acme.com" }
```

<Note>
  Free-tier workspaces are capped at 1 member — invite creation (and member registration
  against a pending invite) is blocked once that cap is hit.
</Note>

### Revoke Invite
<Endpoint method="DELETE" path="/workspace/invites/:invite_id" />

## Long-Lived API Keys

Used for CLI/CI authentication.

### List Auth Keys
<Endpoint method="GET" path="/workspace/auth-keys" />

### Create Auth Key
<Endpoint method="POST" path="/workspace/auth-keys" />

```json
{
    "name": "ci-key"
}
```

<Warning>
  The full key (`kioku_...`) is only shown once at creation. Store it safely.
</Warning>

### Delete Auth Key
<Endpoint method="DELETE" path="/workspace/auth-keys/:key_id" />
