---
title: "Company API"
---
Manage company members, invites, and API keys.

## Company Config

### Get Config
<Endpoint method="GET" path="/company/config" />

### Update Config
<Endpoint method="PUT" path="/company/config" />

```json
{
    "hivemind_enabled": true
}
```

## Members

### List Members
<Endpoint method="GET" path="/company/members" />

### Remove Member
<Endpoint method="DELETE" path="/company/members/:user_id" />

### Update Member Role
<Endpoint method="PUT" path="/company/members/:user_id/:role" />

## Invites

### List Invites
<Endpoint method="GET" path="/company/invites" />

### Create Invite
<Endpoint method="POST" path="/company/invites" />

### Revoke Invite
<Endpoint method="DELETE" path="/company/invites/:invite_id" />

## CLI Auth Keys

Long-lived API keys for CLI authentication.

### List Auth Keys
<Endpoint method="GET" path="/company/auth-keys" />

### Create Auth Key
<Endpoint method="POST" path="/company/auth-keys" />

```json
{
    "name": "ci-key"
}
```

<Warning>
  The full key is only shown once at creation. Store it safely.
</Warning>

### Delete Auth Key
<Endpoint method="DELETE" path="/company/auth-keys/:key_id" />
