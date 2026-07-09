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
kioku knowledge-upload ./report.pdf
# Uploaded report.pdf
```

### Response

```json
{
    "id": "doc-1",
    "filename": "report.pdf",
    "status": "processing"
}
```

Processing happens asynchronously — the PDF text is extracted, chunked, and embedded on the server.

## List Documents

```bash
kioku knowledge-documents
```

## Delete Document

```bash
kioku knowledge-delete doc-1
# Deleted document doc-1
```

Deleting a document removes it and all its embeddings from Qdrant. The knowledge search will no longer return results from that document.