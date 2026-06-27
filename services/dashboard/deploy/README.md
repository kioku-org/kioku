# Kioku Dashboard Deployment

This directory contains the deployment configuration for the Kioku Dashboard at `dashboard.kioku.chat`.

## Prerequisites

1. **Kioku Backend**: The backend services (API gateway, admin API, etc.) must be running
2. **Docker Network**: The `kioku-network` Docker network must exist
3. **Cloudflare Tunnel**: A Cloudflare tunnel must be configured for the subdomain

## Quick Start

### 1. Configure Environment

```bash
cd services/dashboard/deploy
cp .env.example .env
# Edit .env with your configuration
```

### 2. Build and Deploy

```bash
# Build the dashboard image
docker compose build

# Start the dashboard
docker compose up -d
```

### 3. Configure Cloudflare Tunnel

1. Create a Cloudflare tunnel (if not already created):
   ```bash
   cloudflared tunnel create kioku-dashboard
   ```

2. Update `cloudflared.yml` with your tunnel ID

3. Add DNS record for the subdomain:
   ```bash
   cloudflared tunnel route dns kioku-dashboard dashboard.kioku.chat
   ```

4. Run the tunnel:
   ```bash
   cloudflared tunnel run --config cloudflared.yml
   ```

## Environment Variables

### Required

| Variable | Description | Example |
|----------|-------------|---------|
| `VEXA_API_URL` | Internal API URL (Docker network) | `http://vexa-api-gateway:8000` |
| `VEXA_ADMIN_API_KEY` | Admin API key for user management | `your_admin_api_key_here` |

### Optional

| Variable | Description | Default |
|----------|-------------|---------|
| `VEXA_PUBLIC_API_URL` | Public API URL for browser connections | `https://api.kioku.chat` |
| `NEXTAUTH_URL` | NextAuth URL (must match dashboard URL) | `https://dashboard.kioku.chat` |
| `NEXT_PUBLIC_APP_URL` | Public app URL for client-side redirects | `https://dashboard.kioku.chat` |
| `VEXA_ALLOW_DIRECT_LOGIN` | Allow direct email login | `true` |
| `GOOGLE_CLIENT_ID` | Google OAuth client ID | - |
| `GOOGLE_CLIENT_SECRET` | Google OAuth client secret | - |
| `SMTP_HOST` | SMTP server for magic links | - |
| `SMTP_USER` | SMTP username | - |
| `SMTP_PASS` | SMTP password | - |

## Architecture

```
Browser → Cloudflare Tunnel → Dashboard (3001) → API Gateway (8000) → Services
```

The dashboard acts as a proxy:
- Server-side: Proxies API requests to the backend
- Client-side: Uses browser API URL resolution for WebSocket and direct connections

## Verification

After deployment, verify the following:

1. **Dashboard Health**:
   ```bash
   curl -I https://dashboard.kioku.chat/api/health
   ```

2. **Dashboard Login**:
   - Visit `https://dashboard.kioku.chat`
   - Verify the login page loads
   - Test authentication flow

3. **API Connection**:
   - Check that the dashboard can connect to the API
   - Verify WebSocket connections work

## Troubleshooting

### Common Issues

1. **"VEXA_API_URL is required"**: Ensure `VEXA_API_URL` is set in `.env`
2. **CORS errors**: Verify `VEXA_PUBLIC_API_URL` is correct and the API gateway allows the dashboard origin
3. **WebSocket failures**: Check that the API gateway supports WebSocket upgrade on `/ws`
4. **Authentication failures**: Verify `VEXA_ADMIN_API_KEY` matches the admin API configuration

### Logs

```bash
# View dashboard logs
docker compose logs -f dashboard

# Check container status
docker compose ps
```

## Production Considerations

1. **Security**:
   - Use strong, unique API keys
   - Enable OAuth for production (disable direct login)
   - Configure proper CORS origins

2. **Performance**:
   - The dashboard uses Next.js standalone mode for production
   - Static assets are served directly by the container
   - API requests are proxied server-side

3. **Monitoring**:
   - Health check endpoint: `/api/health`
   - Container health check is configured
   - Monitor API gateway connectivity
