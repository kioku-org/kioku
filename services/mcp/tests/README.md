# Kioku MCP Tests

## Unit Tests

Run unit tests:

```bash
cd services/kioku-mcp
pytest tests/ -v
```

## Integration Tests

Integration tests require a running Kioku stack. Set environment variables:

```bash
export KIOKU_API_URL=http://localhost:8056
export KIOKU_API_KEY=your-test-api-key
```

Then run:

```bash
cd services/kioku-mcp
pytest tests/ -v --integration
```

## Test Coverage

- Meeting URL parsing (Google Meet, Zoom)
- API key extraction (Bearer, raw token, X-API-Key header)
- Tool routing and validation
- Error handling
