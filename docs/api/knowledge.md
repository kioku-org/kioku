---
title: "Knowledge API"
---
Vector similarity search across documents, meetings, and ingested sessions.

## Search

<Endpoint method="POST" path="/knowledge/search" />

```json
{
    "query": "deployment strategy",
    "limit": 5
}
```

Returns ranked results from documents, meeting transcripts, and ingested sessions:

```json
[
    {
        "chunk": {
            "text": "We decided to use RunPod for GPU pods...",
            "chunk_type": "transcript",
            "speaker": "Alice",
            "meeting_id": "m-42"
        },
        "meeting": {
            "id": "m-42",
            "title": "Weekly Standup",
            "date": 1700000000000
        },
        "score": 0.95
    }
]
```

<Note>
  Empty queries return an empty array. `limit` is clamped to a minimum of 1. If the
  workspace has no indexed content at all, this returns `[]` immediately without an error.
</Note>

## List Documents

<Endpoint method="GET" path="/knowledge/documents" />

## Upload Document

<Endpoint method="POST" path="/knowledge/documents" />

Multipart form upload. The `file` field accepts **PDF, DOCX, PPTX, TXT, or MD** (50MB cap):

```bash
curl -X POST http://localhost:9100/knowledge/documents \
  -H "Authorization: Bearer $TOKEN" \
  -F "file=@report.pdf"
```

Text is extracted (with an OCR fallback for scanned PDFs), chunked, embedded via Ollama, and stored in Qdrant.

### Response

```json
{
    "id": "doc-1",
    "filename": "report.pdf",
    "status": "processing"
}
```

## Delete Document

<Endpoint method="DELETE" path="/knowledge/documents/:document_id" />

Removes the document and all its embeddings from Qdrant.

## Ingest a Session

<Endpoint method="POST" path="/knowledge/sessions" />

Ingest arbitrary content — a coding session, meeting notes, research notes — via a
paragraph-aware chunker (distinct from the fixed-window chunker used for documents and
meeting transcripts).

```json
{
    "title": "Fix Qdrant gRPC issue",
    "content": "Full session content here...",
    "tags": ["rust", "qdrant"]
}
```

Creates a `coding_sessions` row and chunks are searchable via `/knowledge/search` immediately after.
