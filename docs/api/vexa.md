---
title: "Vexa Proxy API"
---
Hivemind proxies bot requests to the Vexa meeting-bot platform.

## Request Bot

<Endpoint method="POST" path="/vexa/bots" />

Spawns a meeting bot that joins the specified meeting, captures audio, and transcribes.

```json
{
    "platform": "google_meet",
    "native_meeting_id": "abc-defg-hij",
    "bot_name": "Kioku Bot",
    "language": "en",
    "task": "transcribe"
}
```

<Note>
  Requires the full Vexa stack running (meeting-api, runtime-api, transcription-service, vexa-bot).
</Note>

## List Vexa Meetings

<Endpoint method="GET" path="/vexa/meetings" />

Returns meetings recorded by Vexa bots, including their status, transcripts, and recordings.

## Platforms

| Platform | meeting_url example | native_meeting_id |
|---|---|---|
| Google Meet | `https://meet.google.com/abc-defg-hij` | `abc-defg-hij` |
| Zoom | `https://zoom.us/j/123456789?pwd=abc` | `123456789` |
| MS Teams | `https://teams.microsoft.com/l/meetup/...` | full URL |

## Bot Lifecycle

1. **Spawn** — runtime-api creates a bot container (Docker, Process, or RunPod pod)
2. **Join** — bot launches Playwright browser, navigates to meeting URL
3. **Transcribe** — audio captured → Whisper → Redis streams → meeting-api
4. **Exit** — bot leaves meeting (user stop, timeout, or meeting ends)
5. **Ingest** — transcript sent to Hivemind `POST /meetings` → embedded → searchable