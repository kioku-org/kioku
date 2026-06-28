#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEPLOY_DIR="$(dirname "$SCRIPT_DIR")"
cd "$DEPLOY_DIR"

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

info() { echo -e "${GREEN}[SETUP]${NC} $*"; }
warn() { echo -e "${YELLOW}[SETUP]${NC} $*"; }

info "Setting up Kioku Platform deployment..."

# Copy env if not exists
if [[ ! -f .env ]]; then
    info "Creating .env from .env.example..."
    cp .env.example .env
    warn "Please edit .env with your secrets before running 'docker compose up'"
else
    info ".env already exists, skipping"
fi

# Create backup dir
mkdir -p backups

# Generate secure secrets if still default
if grep -q "change-me-to-a-random-64-char-hex-string" .env 2>/dev/null; then
    warn "Default secrets detected. Generating secure ones..."
    JWT_SECRET=$(openssl rand -hex 32)
    ENCRYPTION_SECRET=$(openssl rand -hex 32)
    NEXTAUTH_SECRET=$(openssl rand -base64 32)
    sed -i "s|HIVEMIND_JWT_SECRET=change-me-to-a-random-64-char-hex-string|HIVEMIND_JWT_SECRET=$JWT_SECRET|" .env
    sed -i "s|HIVEMIND_ENCRYPTION_SECRET=change-me-to-a-random-64-char-hex-string|HIVEMIND_ENCRYPTION_SECRET=$ENCRYPTION_SECRET|" .env
    sed -i "s|NEXTAUTH_SECRET=change-me-to-a-random-secret|NEXTAUTH_SECRET=$NEXTAUTH_SECRET|" .env
    info "Secrets generated"
fi

# Detect Docker GID and inject into .env if not set or empty
CURRENT_DOCKER_GID=$(grep "^DOCKER_GID=" .env 2>/dev/null | cut -d= -f2- || true)
if [[ -z "$CURRENT_DOCKER_GID" ]]; then
    DETECTED_GID=$(getent group docker | cut -d: -f3 2>/dev/null || stat -c '%g' /var/run/docker.sock 2>/dev/null || echo "")
    if [[ -n "$DETECTED_GID" ]]; then
        # Replace empty/missing value in-place; append if key doesn't exist yet
        if grep -q "^DOCKER_GID=" .env 2>/dev/null; then
            sed -i "s|^DOCKER_GID=.*|DOCKER_GID=$DETECTED_GID|" .env
        else
            echo "DOCKER_GID=$DETECTED_GID" >> .env
        fi
        info "Detected Docker GID: $DETECTED_GID"
    else
        warn "Could not detect Docker GID — set DOCKER_GID manually in .env if bot spawning fails"
    fi
fi

# Pull pre-built images
info "Pulling pre-built images..."
docker compose -f docker-compose.stateless.yml pull --quiet --ignore-buildable 2>/dev/null \
    || warn "Some images could not be pulled (will be built on first start)"

# Pull the browser-bot image explicitly — it's spawned at runtime by runtime-api-local,
# not listed as a compose service, so the pull above won't fetch it.
BOT_IMAGE=$(grep "^VEXA_BOT_IMAGE=" .env 2>/dev/null | cut -d= -f2- || echo "ghcr.io/kioku-org/kioku-stateless:latest")
BOT_IMAGE="${BOT_IMAGE:-ghcr.io/kioku-org/kioku-stateless:latest}"
info "Pulling browser-bot image ($BOT_IMAGE)..."
docker pull "$BOT_IMAGE" \
    || warn "Could not pull $BOT_IMAGE — bot deployment will fail until this image is available"

info ""
info "Setup complete!"
info ""
info "Next steps:"
info "  1. Review and edit .env with your secrets (API keys, etc.)"
info "  2. Run: ./scripts/manage.sh start"
info "  3. Run: ./scripts/healthcheck.sh"
info ""
info "Default ports:"
info "  Vexa API:         8056"
info "  Vexa Admin:       8057"
info "  Hivemind API:     9100"
info "  Minio Console:    9001"
info "  Vexa MCP:         18888"
