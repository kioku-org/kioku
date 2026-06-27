# Kioku Dashboard

## Overview

The Kioku Dashboard provides a web interface for managing meetings, viewing live transcripts, and accessing your knowledge base.

## Public Instance

The production dashboard is available at **https://dashboard.kioku.chat**

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

### Additional Environment Variables

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `VEXA_API_URL` | Yes | - | Alias for `KIOKU_API_URL` |
| `VEXA_PUBLIC_API_URL` | No | - | Public API URL for browser connections |
| `NEXTAUTH_URL` | No | - | NextAuth URL (must match dashboard URL) |
| `NEXT_PUBLIC_APP_URL` | No | - | Public app URL for client-side redirects |
| `VEXA_ALLOW_DIRECT_LOGIN` | No | `true` | Allow direct email login |
| `VEXA_ADMIN_API_KEY` | No | - | Admin API key for user management |

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

### Production Deployment (dashboard.kioku.chat)

For production deployment to `dashboard.kioku.chat`, see the `deploy/` directory:

```bash
cd services/dashboard/deploy
cp .env.example .env
# Edit .env with your configuration
./deploy.sh build
./deploy.sh start
```

The deployment includes:
- Docker Compose configuration
- Cloudflare Tunnel configuration
- Environment variable templates
- Deployment scripts

See `deploy/README.md` for detailed instructions.

### Kubernetes

See `docs/deployment/kubernetes.md` for Kubernetes deployment instructions.
