#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEPLOY_DIR="$(dirname "$SCRIPT_DIR")"
cd "$DEPLOY_DIR"

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

check_container() {
    local name="$1"
    local status
    status=$(docker inspect --format '{{.State.Status}}' "$name" 2>/dev/null || echo "missing")
    if [[ "$status" == "running" ]]; then
        echo -e "  ${GREEN}✓${NC} $name — running"
        return 0
    else
        echo -e "  ${RED}✗${NC} $name — $status"
        return 1
    fi
}

check_http() {
    local url="$1"
    local name="$2"
    local http_code
    http_code=$(curl -s -o /dev/null -w "%{http_code}" --max-time 5 "$url" 2>/dev/null || echo "000")
    if [[ "$http_code" =~ ^[23] ]]; then
        echo -e "  ${GREEN}✓${NC} $name — HTTP $http_code"
        return 0
    else
        echo -e "  ${RED}✗${NC} $name — HTTP $http_code"
        return 1
    fi
}

echo "═══════════════════════════════════════════"
echo "  Kioku Platform Health Check"
echo "═══════════════════════════════════════════"
echo ""

echo "Stateful containers:"
check_container "postgres"
check_container "qdrant"

echo ""
echo "Stateless containers:"
check_container "kioku-ollama"
check_container "kioku-hivemind"
check_container "kioku-dashboard"
check_container "kioku-runtime-router"
check_container "kioku-mcp"
check_container "kioku-vexa-api-gateway"
check_container "kioku-vexa-admin-api"
check_container "kioku-vexa-meeting-api"
check_container "kioku-vexa-agent-api"
check_container "kioku-vexa-transcription-service"
check_container "kioku-vexa-tts-service"
check_container "kioku-vexa-redis"
check_container "kioku-vexa-minio"

echo ""
echo "HTTP Endpoints:"
check_http "http://localhost:9100/health"      "Hivemind API"
check_http "http://localhost:8056"             "Vexa API Gateway"
check_http "http://localhost:8057"             "Vexa Admin API"
check_http "http://localhost:18888/health"     "Kioku MCP"
check_http "http://localhost:3001/api/health"  "Dashboard"
check_http "http://localhost:9000/minio/health/live" "Minio"

echo ""
echo "═══════════════════════════════════════════"
