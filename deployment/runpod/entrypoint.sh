#!/usr/bin/env bash
set -e

echo "[KIOKU] Starting Docker daemon (dind)..."

# docker:dind handles dockerd startup via its entrypoint
# We override CMD but need to start dockerd ourselves
dockerd-entrypoint.sh dockerd &>/var/log/dockerd.log &
sleep 5

# Wait for Docker to be ready
for i in $(seq 1 30); do
    docker info &>/dev/null 2>&1 && break
    sleep 2
done

docker info &>/dev/null 2>&1 || { echo "[KIOKU] Docker failed to start"; tail -20 /var/log/dockerd.log; exit 1; }
echo "[KIOKU] Docker daemon running"

echo "[KIOKU] Cloning Kioku repository..."
cd /workspace
rm -rf kioku
git clone --branch feat/runpod --depth 1 https://github.com/kioku-org/kioku.git
cd kioku/deployment/docker

echo "[KIOKU] Setting up environment..."
if [[ ! -f .env ]]; then
    cp .env.example .env
    JWT_SECRET=$(openssl rand -hex 32)
    ENCRYPTION_SECRET=$(openssl rand -hex 32)
    sed -i "s|change-me-to-a-random-64-char-hex-string|$JWT_SECRET|g" .env
    sed -i "s|change-me-to-a-random-64-char-hex-string|$ENCRYPTION_SECRET|g" .env
    [[ -n "${DB_NAME:-}" ]] && sed -i "s|^DB_NAME=.*|DB_NAME=$DB_NAME|" .env
    [[ -n "${DB_USER:-}" ]] && sed -i "s|^DB_USER=.*|DB_USER=$DB_USER|" .env
    [[ -n "${DB_PASSWORD:-}" ]] && sed -i "s|^DB_PASSWORD=.*|DB_PASSWORD=$DB_PASSWORD|" .env
fi

echo "[KIOKU] Building and starting platform..."
docker compose up -d --build 2>&1 | tail -20

echo "[KIOKU] Waiting for services..."
sleep 30

echo "[KIOKU] Running health checks..."
check() {
    local code=$(curl -s -o /dev/null -w "%{http_code}" --max-time 5 "$2" 2>/dev/null || echo "000")
    if [[ "$code" =~ ^[23] ]]; then
        echo "  ✓ $1 — HTTP $code"
    else
        echo "  ~ $1 — HTTP $code (may still be starting)"
    fi
}

check "Hivemind" "http://localhost:9100/health"
check "Vexa API" "http://localhost:8056"
check "Qdrant" "http://localhost:6334/collections"
check "Ollama" "http://localhost:11434/api/tags"
check "MinIO" "http://localhost:9001"

PUBLIC_IP=$(curl -s ifconfig.me 2>/dev/null || echo "unknown")
echo ""
echo "═══════════════════════════════════════════════════════"
echo "  Kioku Platform is LIVE at $PUBLIC_IP"
echo "═══════════════════════════════════════════════════════"
echo "  API:       http://$PUBLIC_IP:9100"
echo "  Vexa:      http://$PUBLIC_IP:8056"
echo "  Admin:     http://$PUBLIC_IP:8057"
echo "  Qdrant:    http://$PUBLIC_IP:6334"
echo "  Ollama:    http://$PUBLIC_IP:11434"
echo "  MinIO:     http://$PUBLIC_IP:9001"
echo "  MCP:       http://$PUBLIC_IP:18888"
echo "═══════════════════════════════════════════════════════"

# Keep container alive
echo "[KIOKU] Platform running. Keeping alive..."
tail -f /var/log/dockerd.log
