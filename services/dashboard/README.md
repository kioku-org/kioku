# Kioku Dashboard

## Overview

The Kioku Dashboard provides a web interface for managing meetings, viewing live transcripts, and accessing your knowledge base.

## Quick Start

### Docker

```bash
docker run --rm -p 3001:3001 \
  -e KIOKU_API_URL=http://your-kioku-host:8056 \
  ghcr.io/kioku-org/kioku-dashboard:latest
```

### Local Development

```bash
cd services/kioku-dashboard
npm install
npm run dev
```

The dashboard will be available at `http://localhost:3001`.

## Environment Variables

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `KIOKU_API_URL` | Yes | - | URL of the Kioku API gateway (e.g., `http://localhost:8056`) |
| `NEXT_PUBLIC_BASE_PATH` | No | - | Base path for sub-path deployments |
| `NEXT_PUBLIC_DOCS_URL` | No | `https://docs.kioku.chat` | URL for documentation links |

## Features

- **Meeting Management**: Launch bots into meetings, view active/past meetings
- **Live Transcripts**: Real-time transcript viewing with speaker attribution
- **Recordings**: Audio playback synced with transcript segments
- **Settings**: API token management, webhook configuration
- **Knowledge Base**: Search and browse your knowledge base

## Testing

### Unit Tests

```bash
npm run test
```

### Build

```bash
npm run build
```

## Architecture

The dashboard is a Next.js application that communicates with the Kioku API gateway. It does not access the database directly—all operations go through the API.

```
Browser → Dashboard (3001) → API Gateway (8056) → Services
```

## Deployment

### Docker Compose

```yaml
services:
  dashboard:
    image: ghcr.io/kioku-org/kioku-dashboard:latest
    ports:
      - "3001:3001"
    environment:
      - KIOKU_API_URL=http://api-gateway:8056
    depends_on:
      - api-gateway
```

### Kubernetes

See `docs/deployment/kubernetes.md` for Kubernetes deployment instructions.
