---
title: "Meeting Transcripts"
---
Import meeting transcripts to make them searchable.

## Ingest

### REST API

```bash
curl -X POST http://localhost:9100/meetings \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "title": "Weekly Standup",
    "date": 1700000000000,
    "duration_seconds": 1800,
    "participants": ["Alice", "Bob", "Charlie"],
    "transcript": [
      {"speaker": "Alice", "text": "Lets review the deployment plan.", "start_time": 0, "end_time": 5},
      {"speaker": "Bob", "text": "RunPod is working well for GPU pods.", "start_time": 5, "end_time": 10}
    ]
  }'
```

Each transcript segment is embedded individually, preserving speaker attribution and timestamps in the vector store metadata.

## Via Vexa Bot

For live meetings, spawn a Vexa bot that captures and transcribes automatically:

```bash
curl -X POST http://localhost:9100/vexa/bots \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "platform": "google_meet",
    "native_meeting_id": "abc-defg-hij",
    "task": "transcribe"
  }'
```

The bot joins, transcribes, and on exit, the transcript is automatically ingested.

## List Meetings

```bash
kioku meetings-list
# m-1  Weekly Standup  1700000000000
# m-2  Design Review   1700100000000
```

## Search Across Meetings

Meeting transcripts are searchable alongside documents:

```bash
kioku knowledge-search "what did we decide about RunPod"
# r-1 [score=0.92]: We decided to use RunPod for GPU pods...
```