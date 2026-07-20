---
title: "Vexa Proxy API"
---
Hivemind proxies bot requests to the Vexa meeting-bot platform, and lazily provisions each
Hivemind user their own linked Vexa credential the first time they use one of these routes
— see [Vexa ↔ Hivemind credential linking](/architecture/vexa-hivemind-credentials).

## Request Bot

<Endpoint method="POST" path="/vexa/bots" />

Spawns a meeting bot that joins the specified meeting, captures audio, and transcribes.

```json
{
    "platform": "google_meet",
    "native_meeting_id": "abc-defg-hij",
    "bot_name": "Kioku Bot",
    "language": "en"
}
```

<Note>
  Requires the full Vexa stack running (meeting-api, runtime-api, transcription, vexa-bot)
  — always true in the default `kioku-stateful` deployment.
</Note>

## Bot Status

<Endpoint method="GET" path="/vexa/bots/status" />

List currently running bots for the workspace.

## Stop Bot

<Endpoint method="DELETE" path="/vexa/bots/:platform/:native_meeting_id" />

## List Vexa Meetings

<Endpoint method="GET" path="/vexa/meetings" />

Returns meetings recorded by Vexa bots, including their status, transcripts, and recordings.

## Vexa Token Exchange

<Endpoint method="GET" path="/vexa/token" />

Resolves the caller's Kioku credential (JWT, `kioku_...` key, or session token) into their
own per-user Vexa API key. Used internally by the MCP service so one Kioku credential
works against both the knowledge and meeting toolsets — see [MCP overview](/mcp/overview).

## Platforms

| Platform | meeting_url example | native_meeting_id |
|---|---|---|
| Google Meet | `https://meet.google.com/abc-defg-hij` | `abc-defg-hij` |
| Zoom | `https://zoom.us/j/123456789?pwd=abc` | `123456789` |
| MS Teams | `https://teams.microsoft.com/l/meetup/...` | full URL |

## Bot Lifecycle

1. **Spawn** — runtime-api creates a bot pod (Docker container locally, or a RunPod pod)
2. **Join** — bot launches Playwright, navigates to the meeting URL
3. **Transcribe** — audio captured → embedded transcription service (kiku/whisper.cpp, or cloud STT via OpenRouter) → Redis stream → meeting-api
4. **Alone-detection** — Google Meet and MS Teams auto-leave after a configurable timeout
   once alone in the meeting; Zoom has no alone-detection yet
5. **Exit** — bot leaves (user stop, alone-timeout, or max-duration timeout)
6. **Ingest** — transcript sent to Hivemind `POST /meetings` → embedded → searchable

See [Vexa](/architecture/vexa) for the full architecture, including the RunPod warm-pool
and orphan-cleanup behavior.
