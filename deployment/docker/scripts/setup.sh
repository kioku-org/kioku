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
    sed -i "s|change-me-to-a-random-64-char-hex-string|$JWT_SECRET|g" .env
    sed -i "s|change-me-to-a-random-64-char-hex-string|$ENCRYPTION_SECRET|g" .env
    info "Secrets updated"
fi

# Pull images
info "Pulling base images..."
docker compose pull --quiet 2>/dev/null || warn "Some images may need to be built"

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
