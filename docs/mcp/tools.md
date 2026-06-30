---
title: "MCP Tools"
---
The Kioku MCP server exposes these tools to AI clients.

## search

Search the knowledge base (documents + meetings) with semantic similarity.

```json
{
    "tool": "search",
    "arguments": {
        "query": "deployment strategy",
        "limit": 5
    }
}
```

Returns ranked results with `chunk` (text, speaker, chunk_type), `meeting` (id, title, date), and `score`.

## meetings

List all meetings in the company.

```json
{
    "tool": "meetings",
    "arguments": {}
}
```

Returns meetings with `id`, `title`, `date`, `duration_seconds`, `participants`.

## transcript

Get the full transcript of a specific meeting.

```json
{
    "tool": "transcript",
    "arguments": {
        "meeting_id": "m-1"
    }
}
```

Returns transcript segments with `speaker`, `text`, `start_time`, `end_time`.

## meeting_get

Get details of a specific meeting.

```json
{
    "tool": "meeting_get",
    "arguments": {
        "meeting_id": "m-1"
    }
}
```

## documents

List all uploaded documents.

```json
{
    "tool": "documents",
    "arguments": {}
}
```

## document_delete

Delete a document from the knowledge base.

```json
{
    "tool": "document_delete",
    "arguments": {
        "document_id": "doc-1"
    }
}
```

## meeting

Ingest a meeting transcript into the knowledge base.

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

## session

Ingest a coding or working session into the knowledge base.

```json
{
    "tool": "session",
    "arguments": {
        "title": "Fix Qdrant gRPC issue",
        "summary": "Identified that qdrant-client uses gRPC but Qdrant had no grpc_port configured. Added grpc_port: 6335 and pointed Hivemind at it.",
        "decisions": ["Use gRPC port 6335 for Qdrant", "INTERNAL_API_SECRET is the shared secret for service-to-service calls"],
        "tags": ["rust", "qdrant", "hivemind", "docker"]
    }
}
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

### "Upload this meeting"

Ask Claude:
> Ingest this meeting transcript: [paste transcript]

Claude calls `meeting` with the structured data.

### "Store this coding session"

Ask Claude:
> Save what we just worked on to my knowledge base.

Claude calls `session` with a summary of the session, decisions made, and tags.
