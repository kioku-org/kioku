#!/usr/bin/env bash
set -euo pipefail

# ═══════════════════════════════════════════════════════════════════════════════
# Kioku Platform - RunPod Pod Initializer
# Runs inside the Pod to install Docker and deploy the full platform
# ═══════════════════════════════════════════════════════════════════════════════

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

info()  { echo -e "${GREEN}[KIOKU]${NC} $*"; }
warn()  { echo -e "${YELLOW}[KIOKU]${NC} $*"; }
error() { echo -e "${RED}[KIOKU]${NC} $*"; exit 1; }

WORKSPACE="/workspace"
REPO_URL="https://github.com/kioku-app/kioku.git"
BRANCH="feat/runpod"

cd "$WORKSPACE"

# ─── 1. Install Docker ───────────────────────────────────────────────────────
info "Installing Docker..."

if ! command -v docker &>/dev/null; then
    apt-get update
    apt-get install -y ca-certificates curl gnupg
    
    install -m 0755 -d /etc/apt/keyrings
    curl -fsSL https://download.docker.com/linux/ubuntu/gpg | gpg --dearmor -o /etc/apt/keyrings/docker.gpg
    chmod a+r /etc/apt/keyrings/docker.gpg
    
    echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.gpg] https://download.docker.com/linux/ubuntu $(. /etc/os-release && echo "$VERSION_CODENAME") stable" > /etc/apt/sources.list.d/docker.list
    
    apt-get update
    apt-get install -y docker-ce docker-ce-cli containerd.io docker-compose-plugin
    
    info "Docker installed: $(docker --version)"
else
    info "Docker already installed: $(docker --version)"
fi

# ─── 2. Start Docker daemon ──────────────────────────────────────────────────
info "Starting Docker daemon..."
dockerd &>/var/log/dockerd.log &
sleep 3
docker info &>/dev/null || { sleep 5; docker info &>/dev/null; } || error "Docker daemon failed to start"
info "Docker daemon running"

# ─── 3. Clone/pull repo ──────────────────────────────────────────────────────
if [[ -d "$WORKSPACE/kioku" ]]; then
    info "Pulling latest changes..."
    cd "$WORKSPACE/kioku"
    git fetch origin "$BRANCH" 2>/dev/null || true
    git checkout "$BRANCH" 2>/dev/null || git checkout -b "$BRANCH" "origin/$BRANCH" 2>/dev/null || true
    git pull origin "$BRANCH" 2>/dev/null || true
else
    info "Cloning repository..."
    git clone --branch "$BRANCH" --depth 1 "$REPO_URL" "$WORKSPACE/kioku" 2>/dev/null || \
    git clone --branch main --depth 1 "$REPO_URL" "$WORKSPACE/kioku"
    cd "$WORKSPACE/kioku"
fi

info "Repository ready at $WORKSPACE/kioku"

# ─── 4. Setup environment ────────────────────────────────────────────────────
DEPLOY_DIR="$WORKSPACE/kioku/deployment/docker"
cd "$DEPLOY_DIR"

if [[ ! -f .env ]]; then
    info "Creating .env from template..."
    cp .env.example .env
    
    # Generate secrets
    JWT_SECRET=$(openssl rand -hex 32)
    ENCRYPTION_SECRET=$(openssl rand -hex 32)
    sed -i "s|change-me-to-a-random-64-char-hex-string|$JWT_SECRET|g" .env
    sed -i "s|change-me-to-a-random-64-char-hex-string|$ENCRYPTION_SECRET|g" .env
    
    # Use env vars from RunPod if available
    [[ -n "${DB_NAME:-}" ]] && sed -i "s|^DB_NAME=.*|DB_NAME=$DB_NAME|" .env
    [[ -n "${DB_USER:-}" ]] && sed -i "s|^DB_USER=.*|DB_USER=$DB_USER|" .env
    [[ -n "${DB_PASSWORD:-}" ]] && sed -i "s|^DB_PASSWORD=.*|DB_PASSWORD=$DB_PASSWORD|" .env
fi

info "Environment configured"

# ─── 5. Build and start services ─────────────────────────────────────────────
info "Building and starting Kioku Platform..."
info "This will take several minutes on first run..."

docker compose up -d --build 2>&1 | tail -20

info "Waiting for services to initialize..."
sleep 15

# ─── 6. Health check ─────────────────────────────────────────────────────────
info "Running health checks..."

check_endpoint() {
    local name="$1" url="$2"
    local code
    code=$(curl -s -o /dev/null -w "%{http_code}" --max-time 5 "$url" 2>/dev/null || echo "000")
    if [[ "$code" =~ ^[23] ]]; then
        echo -e "  ${GREEN}✓${NC} $name — HTTP $code"
    else
        echo -e "  ${YELLOW}~${NC} $name — HTTP $code (may still be starting)"
    fi
}

check_endpoint "Hivemind API" "http://localhost:9100/health"
check_endpoint "Vexa API Gateway" "http://localhost:8056"
check_endpoint "Vexa Admin API" "http://localhost:8057"
check_endpoint "Qdrant" "http://localhost:6334/collections"
check_endpoint "Ollama" "http://localhost:11434/api/tags"
check_endpoint "MinIO Console" "http://localhost:9001"
check_endpoint "Vexa MCP" "http://localhost:18888"

# ─── 7. Summary ──────────────────────────────────────────────────────────────
PUBLIC_IP=$(curl -s ifconfig.me 2>/dev/null || echo "unknown")

echo ""
info "═══════════════════════════════════════════════════════════════"
info "  Kioku Platform Deployed on RunPod!"
info "═══════════════════════════════════════════════════════════════"
info ""
info "  Public IP: $PUBLIC_IP"
info ""
info "  API Endpoints:"
info "    Hivemind:   http://$PUBLIC_IP:9100"
info "    Vexa API:    http://$PUBLIC_IP:8056"
info "    Admin API:   http://$PUBLIC_IP:8057"
info "    Qdrant:      http://$PUBLIC_IP:6334"
info "    Ollama:      http://$PUBLIC_IP:11434"
info "    MinIO:       http://$PUBLIC_IP:9001"
info "    MCP:         http://$PUBLIC_IP:18888"
info ""
info "  Health: http://$PUBLIC_IP:9100/health"
info ""
info "  Manage:"
info "    Logs:     docker compose -f $DEPLOY_DIR/docker-compose.yml logs -f"
info "    Status:   docker compose -f $DEPLOY_DIR/docker-compose.yml ps"
info "    Stop:     docker compose -f $DEPLOY_DIR/docker-compose.yml down"
info "═══════════════════════════════════════════════════════════════"

info "Kioku Platform is live!"
