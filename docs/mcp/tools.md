---
title: "MCP Tools"
---
The Kioku MCP server exposes these tools to AI clients.

## kioku_search

Search the knowledge base (documents + meetings) with semantic similarity.

```json
{
    "tool": "kioku_search",
    "arguments": {
        "query": "deployment strategy",
        "limit": 5
    }
}
```

Returns ranked results with `chunk` (text, speaker, chunk_type), `meeting` (id, title, date), and `score`.

## kioku_list_meetings

List all meetings in the company.

```json
{
    "tool": "kioku_list_meetings",
    "arguments": {}
}
```

Returns meetings with `id`, `title`, `date`, `duration_seconds`, `participants`.

## kioku_get_transcript

Get the full transcript of a specific meeting.

```json
{
    "tool": "kioku_get_transcript",
    "arguments": {
        "meeting_id": "m-1"
    }
}
```

Returns transcript segments with `speaker`, `text`, `start_time`, `end_time`.

## kioku_get_meeting

Get details of a specific meeting.

```json
{
    "tool": "kioku_get_meeting",
    "arguments": {
        "meeting_id": "m-1"
    }
}
```

## kioku_list_documents

List all uploaded documents.

```json
{
    "tool": "kioku_list_documents",
    "arguments": {}
}
```

## kioku_delete_document

Delete a document from the knowledge base.

```json
{
    "tool": "kioku_delete_document",
    "arguments": {
        "document_id": "doc-1"
    }
}
```

## kioku_ingest_meeting

Ingest a meeting transcript into the knowledge base.

```json
{
    "tool": "kioku_ingest_meeting",
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

## Use Cases

### "What did we discuss last week?"

Ask Claude:
> Search my knowledge base for discussions from last week about the roadmap.

Claude calls `kioku_search` with query "roadmap" and returns relevant meeting excerpts.

### "Summarize the last standup"

Ask Claude:
> Get the transcript of my last standup and summarize it.

Claude calls `kioku_list_meetings`, finds the latest, then calls `kioku_get_transcript`.

### "Upload this meeting"

Ask Claude:
> Ingest this meeting transcript: [paste transcript]

Claude calls `kioku_ingest_meeting` with the structured data.