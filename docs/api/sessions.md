---
title: "Sessions API"
---
Sessions are conversation containers with messages and execution traces.

## List Sessions

<Endpoint method="GET" path="/sessions" />

Returns all sessions for the authenticated user's company.

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