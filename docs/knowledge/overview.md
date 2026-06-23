---
title: "Knowledge Pipeline"
---
How Kioku turns raw data into searchable knowledge.

## Embedding Model

Kioku uses `nomic-embed-text-v2-moe` via Ollama for all embeddings.

| Metric | Value |
|---|---|
| MTEB score | 63.9 |
| Dimensions | 256–768 (configurable) |
| Latency (GPU) | 5–20ms |
| Latency (CPU) | 50–200ms |
| Cost | Free (compute only) |
| Privacy | Data stays on your server |

This model matches OpenAI's `text-embedding-3-small` (62.3 MTEB) on benchmarks while running entirely on your hardware.

## Pipeline

### Documents (PDF)

```
Upload PDF → pdf-extract (text) → chunk → Ollama embed → Qdrant store
```

1. PDF uploaded via `POST /knowledge/documents`
2. Text extracted using `pdf-extract` crate
3. Text split into chunks
4. Each chunk embedded via Ollama HTTP API
5. Embeddings + metadata stored in Qdrant

### Meetings (Transcripts)

```
Meeting transcript → per-segment embed → Qdrant store
```

1. Transcript ingested via `POST /meetings`
2. Each transcript segment (speaker + text + timestamps) embedded
3. Embeddings + meeting metadata stored in Qdrant
4. Searchable via `POST /knowledge/search`

## Search

Vector similarity search across all knowledge (documents + meetings):

```bash
curl -X POST http://localhost:9100/knowledge/search \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"query":"deployment strategy","limit":5}'
```

Results are ranked by semantic similarity score (0–1).