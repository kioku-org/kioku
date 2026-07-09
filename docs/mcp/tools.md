---
title: "MCP Tools"
---
The Kioku MCP server (`kioku-mcp`) exposes 25 tools to AI clients: 8 knowledge tools
(proxied to Hivemind) and 17 meeting/bot tools (proxied to the Vexa gateway). See
[MCP overview](/mcp/overview) for the auth model.

## Knowledge tools

### search

Semantic search across documents, meetings, and ingested sessions.

```json
{
    "tool": "search",
    "arguments": {
        "query": "deployment strategy",
        "limit": 6
    }
}
```

`limit` defaults to 6, clamped between 1 and 20. Returns ranked results with `chunk` (text, speaker, chunk_type), `meeting` (id, title, date), and `score`.

### meetings

List all meetings in the workspace. No arguments.

### meeting_get

Get details of a specific meeting.

```json
{ "tool": "meeting_get", "arguments": { "meeting_id": "m-1" } }
```

### transcript

Get the full transcript of a specific meeting.

```json
{ "tool": "transcript", "arguments": { "meeting_id": "m-1" } }
```

Returns transcript segments with `speaker`, `text`, `start_time`, `end_time`.

### documents

List all uploaded documents. No arguments.

### document_delete

Delete a document from the knowledge base.

```json
{ "tool": "document_delete", "arguments": { "document_id": "doc-1" } }
```

### meeting

Ingest a raw meeting transcript into the knowledge base.

```json
{
    "tool": "meeting",
    "arguments": {
        "title": "Planning Meeting",
        "date": 1700000000000,
        "duration_seconds": 3600,
        "participants": ["Alice", "Bob"],
        "transcript": [
            {"speaker": "Alice", "text": "Lets plan the roadmap.", "start_time": 0, "end_time": 10}
        ]
    }
}
```

### session

Ingest a coding or working session into the knowledge base.

```json
{
    "tool": "session",
    "arguments": {
        "title": "Fix Qdrant gRPC issue",
        "summary": "Identified that qdrant-client uses gRPC but Qdrant had no grpc_port configured.",
        "decisions": ["Use gRPC port 6335 for Qdrant"],
        "tags": ["rust", "qdrant", "hivemind"]
    }
}
```

## Meeting / bot tools

### parse_meeting_link

Parse a meeting URL into platform, native meeting id, and passcode — a pure local parse, no HTTP call.

```json
{ "tool": "parse_meeting_link", "arguments": { "meeting_url": "https://meet.google.com/abc-defg-hij" } }
```

### request_meeting_bot

Spawn a bot for a live meeting. Idempotent: a 409 (already running) resolves to
`{"status": "already_exists", "meeting": {...}}` instead of erroring.

```json
{
    "tool": "request_meeting_bot",
    "arguments": {
        "meeting_platform": "google_meet",
        "native_meeting_id": "abc-defg-hij",
        "meeting_url": "https://meet.google.com/abc-defg-hij",
        "bot_name": "Kioku Bot",
        "language": "en"
    }
}
```

### get_meeting_transcript

Real-time transcript for a live (in-progress) meeting.

```json
{ "tool": "get_meeting_transcript", "arguments": { "meeting_id": "abc-defg-hij" } }
```

### get_bot_status

Status of all currently running bots. No required arguments.

### update_bot_config

Update an active bot's config (e.g. transcription language) mid-meeting.

```json
{ "tool": "update_bot_config", "arguments": { "meeting_id": "abc-defg-hij", "language": "es" } }
```

### stop_bot

Remove a bot from a meeting.

```json
{ "tool": "stop_bot", "arguments": { "meeting_id": "abc-defg-hij" } }
```

### list_meetings

List Vexa meetings, paginated and filterable.

```json
{ "tool": "list_meetings", "arguments": { "limit": 20, "status": "completed", "platform": "google_meet" } }
```

### update_meeting_data

Update a meeting's metadata.

### delete_meeting

Purge/anonymize a finalized meeting.

### list_recordings

```json
{ "tool": "list_recordings", "arguments": { "limit": 20, "offset": 0 } }
```

### get_recording / delete_recording

```json
{ "tool": "get_recording", "arguments": { "recording_id": "r-1" } }
```

### get_recording_media_download

Get a download URL for a specific recording's media file (rewrites relative/internal MinIO URLs to a public gateway URL).

```json
{ "tool": "get_recording_media_download", "arguments": { "recording_id": "r-1", "media_file_id": "m-1" } }
```

### get_recording_config / update_recording_config

Get or update the caller's default recording configuration.

### get_meeting_bundle

The efficient one-call option: transcript + recordings + a share link together.

```json
{
    "tool": "get_meeting_bundle",
    "arguments": {
        "meeting_id": "abc-defg-hij",
        "include_recordings": true,
        "include_share_link": true
    }
}
```

`include_recordings` and `include_share_link` default to `true`; `include_segments` and `include_media_download_urls` default to `false`.

### create_transcript_share_link

Create a short-lived public URL for a meeting's transcript.

```json
{ "tool": "create_transcript_share_link", "arguments": { "meeting_id": "abc-defg-hij", "ttl_seconds": 3600 } }
```

## Use Cases

### "What did we discuss last week?"

Ask Claude:
> Search my knowledge base for discussions from last week about the roadmap.

Claude calls `search` with query "roadmap" and returns relevant meeting excerpts.

### "Summarize the last standup"

Ask Claude:
> Get the transcript of my last standup and summarize it.

Claude calls `meetings`, finds the latest, then calls `transcript`.

### "Join this meeting and prep me"

Ask Claude:
> Join https://meet.google.com/abc-defg-hij and give me a bundle summary when it's done.

Claude uses the `vexa.meeting_prep` prompt (parse link → `request_meeting_bot`), and later
the `vexa.post_meeting` prompt (`get_meeting_bundle` → summary/decisions/action items).

### "Store this coding session"

Ask Claude:
> Save what we just worked on to my knowledge base.

Claude calls `session` with a summary of the session, decisions made, and tags.
