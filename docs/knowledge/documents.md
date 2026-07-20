---
title: "Documents"
---
Upload and manage documents (PDF, DOCX, PPTX, TXT, or MD) in your knowledge base.

## Upload

### REST API

```bash
curl -X POST http://localhost:9100/knowledge/documents \
  -H "Authorization: Bearer $TOKEN" \
  -F "file=@report.pdf"
```

### CLI

```bash
kioku docs report.pdf
# Uploaded report.pdf
```

### Response

```json
{
    "id": "doc-1",
    "filename": "report.pdf",
    "status": "completed"
}
```

Processing is synchronous — the request blocks while the server extracts, chunks, and embeds the text, and only returns once `status` is `completed` (or the upload is rejected/marked `empty` if no text was found).

## List Documents

```bash
kioku docs
```

## Delete Document

```bash
kioku docs --delete doc-1
# Deleted document doc-1
```

Deleting a document removes it and all its embeddings from Qdrant. The knowledge search will no longer return results from that document.