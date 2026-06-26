# Kioku MCP Service

## Overview

The Kioku MCP (Model Context Protocol) service exposes Kioku's meeting and knowledge capabilities as MCP tools that AI assistants can use.

## Quick Start

### Docker

```bash
docker run --rm -p 18888:18888 \
  -e KIOKU_API_URL=http://your-kioku-host:8056 \
  ghcr.io/kioku-org/kioku-mcp:latest
```

### Local Development

```bash
cd services/kioku-mcp
pip install -r requirements.txt
python main.py
```

The MCP service will be available at `http://localhost:18888`.

## Environment Variables

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `KIOKU_API_URL` | No | `http://api-gateway:8000` | URL of the Kioku API gateway |
| `KIOKU_ENV` | No | `development` | Environment (development/production) |

## MCP Tools

The service provides 32+ MCP tools organized by category:

| Category | Count | Examples |
|----------|-------|---------|
| Meeting Management | 7 | `request_meeting_bot`, `stop_bot`, `list_meetings` |
| Transcripts | 3 | `get_meeting_transcript`, `create_transcript_share_link` |
| Recordings | 6 | `list_recordings`, `get_recording` |
| Bot Control | 7 | `send_chat_message`, `bot_speak`, `bot_screen_share` |
| Calendar | 5 | `calendar_connect`, `list_calendar_events` |

## Configuration

### Claude Desktop / Claude Code

Add to your MCP client config:

```json
{
  "mcpServers": {
    "Kioku": {
      "url": "http://localhost:18888/mcp",
      "headers": {
        "Authorization": "Bearer YOUR_API_KEY"
      }
    }
  }
}
```

## Testing

### Unit Tests

```bash
pip install pytest
pytest tests/ -v
```

### Integration Tests

```bash
export KIOKU_API_URL=http://localhost:8056
export KIOKU_API_KEY=your-test-api-key
pytest tests/ -v --integration
```

## Architecture

The MCP service is a FastAPI application that proxies requests to the Kioku API gateway. It translates MCP tool calls into REST API requests.

```
AI Client → MCP Service (18888) → API Gateway (8056) → Services
```
