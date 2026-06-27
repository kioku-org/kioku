---
title: "Knowledge API"
---
Vector similarity search across documents and meetings.

## Search

<Endpoint method="POST" path="/knowledge/search" />

```json
{
    "query": "deployment strategy",
    "limit": 5
}
```

Returns ranked results from both documents and meeting transcripts:

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
  Empty queries return an empty array. The `limit` is clamped to a minimum of 1.
</Note>

## List Documents

<Endpoint method="GET" path="/knowledge/documents" />

## Upload Document (PDF)

<Endpoint method="POST" path="/knowledge/documents" />

Multipart form upload. The `file` field must be a PDF:

```bash
curl -X POST http://localhost:9100/knowledge/documents \
  -H "Authorization: Bearer $TOKEN" \
  -F "file=@report.pdf"
```

Text is extracted, chunked, embedded via Ollama, and stored in Qdrant.

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