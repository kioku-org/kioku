---
title: "Sessions API"
description: "Store conversation sessions, messages, and trace steps in your workspace."
---

Sessions are conversation containers with messages and execution traces. Every route requires
an authenticated Kioku credential and is scoped to the active workspace and calling user.

Use `X-Workspace-Id` to select a non-default workspace. See [Authentication](/api/authentication).

## List sessions

<Endpoint method="GET" path="/sessions" />

Returns the calling user's sessions in the active workspace.

## Create Session

<Endpoint method="POST" path="/sessions" />

```json
{
    "title": "Research Session",
    "mode": "research"
}
```

## Get Session

<Endpoint method="GET" path="/sessions/:session_id" />

## Update Session

<Endpoint method="PATCH" path="/sessions/:session_id" />

```json
{
    "title": "Updated Title"
}
```

## Delete Session

<Endpoint method="DELETE" path="/sessions/:session_id" />

## List Messages

<Endpoint method="GET" path="/sessions/:session_id/messages" />

## Send Message

<Endpoint method="POST" path="/sessions/:session_id/messages" />

Messages use OpenAI chat format with multi-part content:

```json
{
    "id": "uuid-v4",
    "role": "user",
    "content": [
        {
            "type": "text",
            "text": "What was discussed in the last standup?"
        }
    ],
    "timestamp": 1700000000000
}
```

## List Traces

<Endpoint method="GET" path="/sessions/:session_id/traces" />

## Create Trace

<Endpoint method="POST" path="/sessions/:session_id/traces" />

## Update Trace

<Endpoint method="PATCH" path="/sessions/:session_id/traces/:trace_id" />