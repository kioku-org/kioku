---
title: "Knowledge Search"
---
Search across all your knowledge base — documents and meetings — with semantic similarity.

## How It Works

1. Query text is embedded using the same Ollama model (`nomic-embed-text-v2-moe`)
2. Embedding is sent to Qdrant for vector similarity search
3. Results are filtered by company ID (multi-tenant isolation)
4. Top N results returned with similarity scores

## REST API

```bash
curl -X POST http://localhost:9100/knowledge/search \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"query":"what was decided about deployment","limit":5}'
```

### Response

```json
[
    {
        "id": "r-1",
        "meeting_id": "m-42",
        "text": "We decided to use RunPod for GPU pods and keep the stateful services on CPU.",
        "speaker": "Alice",
        "score": 0.92,
        "metadata": null
    }
]
```

## CLI

```bash
kioku knowledge-search "deployment strategy"
# r-1 [score=0.920]: We decided to use RunPod for GPU pods...
# r-2 [score=0.845]: The stateful pod runs on CPU...
```

## MCP

AI clients can search knowledge via the `kioku_search` MCP tool:

```json
{
    "tool": "kioku_search",
    "arguments": {
        "query": "deployment strategy",
        "limit": 5
    }
}
```

## Notes

- Empty queries return an empty array (no error)
- `limit` is clamped to minimum 1
- Results are scoped to the authenticated user's company
- Both documents and meetings are searched in a single query