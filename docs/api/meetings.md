---
title: "Meetings API"
---
Ingest meeting transcripts to make them searchable knowledge, and manage live meeting bots.

## List Meetings

<Endpoint method="GET" path="/meetings" />

Returns all meetings for the workspace.

## Get Meeting

<Endpoint method="GET" path="/meetings/:meeting_id" />

## Get Transcript

<Endpoint method="GET" path="/meetings/:meeting_id/transcript" />

Returns the stored transcript chunks for a meeting.

## Ingest Meeting

<Endpoint method="POST" path="/meetings" />

Ingest a transcript. The row is written synchronously; embedding happens in the
background, so the response returns before the transcript is searchable.

```json
{
    "title": "Weekly Standup",
    "date": 1700000000000,
    "duration_seconds": 600,
    "participants": ["Alice", "Bob"],
    "transcript": [
        {
            "speaker": "Alice",
            "text": "Let's discuss the deployment plan.",
            "start_time": 0,
            "end_time": 5
        },
        {
            "speaker": "Bob",
            "text": "I think RunPod is the way to go.",
            "start_time": 5,
            "end_time": 12
        }
    ]
}
```

## Request a Bot (live meeting)

<Endpoint method="POST" path="/vexa/bots" />

Proxy to Vexa meeting-api to spawn a bot for a live meeting. `bot_name` is auto-filled
from the workspace name if omitted. Requires the Vexa stack running (always true in the
default `kioku-stateful` deployment).

```json
{
    "platform": "google_meet",
    "native_meeting_id": "abc-defg-hij",
    "bot_name": "Kioku Bot",
    "language": "en"
}
```

## Bot Status

<Endpoint method="GET" path="/vexa/bots/status" />

List currently running bots for the workspace.

## Stop a Bot

<Endpoint method="DELETE" path="/vexa/bots/:platform/:native_meeting_id" />

## List Vexa Meetings

<Endpoint method="GET" path="/vexa/meetings" />

Proxy to Vexa meeting-api. Returns meetings recorded by Vexa bots.
