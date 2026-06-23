---
title: "Meetings API"
---
Ingest meeting transcripts to make them searchable knowledge.

## List Meetings

<Endpoint method="GET" path="/meetings" />

Returns all meetings for the company.

## Ingest Meeting

<Endpoint method="POST" path="/meetings" />

Ingest a transcript. Each segment is embedded and stored in Qdrant.

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

## Request Vexa Bot

<Endpoint method="POST" path="/vexa/bots" />

Proxy to Vexa meeting-api to spawn a bot for a live meeting. Requires Vexa stack running.

## List Vexa Meetings

<Endpoint method="GET" path="/vexa/meetings" />

Proxy to Vexa meeting-api. Returns meetings recorded by Vexa bots.