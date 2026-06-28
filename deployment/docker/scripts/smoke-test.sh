#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEPLOY_DIR="$(dirname "$SCRIPT_DIR")"

STATEFUL_FILE="$DEPLOY_DIR/docker-compose.stateful.yml"
COMPOSE_FILE="$DEPLOY_DIR/docker-compose.stateless.yml"
ENV_FILE="$DEPLOY_DIR/.env"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

WAIT_SECONDS=60
SKIP_DEPLOY=false

for arg in "$@"; do
    case $arg in
        --wait) WAIT_SECONDS="$2"; shift 2 ;;
        --skip-deploy) SKIP_DEPLOY=true; shift ;;
        --help) echo "Usage: $0 [--wait SECONDS] [--skip-deploy]"; exit 0 ;;
    esac
done

log()  { echo -e "${GREEN}[PASS]${NC} $1"; }
warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
fail() { echo -e "${RED}[FAIL]${NC} $1"; FAIL_COUNT=$((FAIL_COUNT + 1)); }

FAIL_COUNT=0

#####################################
# 0. Prerequisites
#####################################

echo "========================================="
echo "  Kioku Server Smoke Test"
echo "========================================="
echo ""

command -v docker >/dev/null 2>&1 || { fail "docker not found"; exit 1; }
command -v docker compose >/dev/null 2>&1 || { fail "docker compose not found"; exit 1; }
command -v curl >/dev/null 2>&1 || { fail "curl not found"; exit 1; }

log "Prerequisites: docker, docker compose, curl all available"

#####################################
# 1. Deploy stack
#####################################

if [ "$SKIP_DEPLOY" = false ]; then
    echo ""
    echo "--- Starting stack ---"
    if [ ! -f "$ENV_FILE" ]; then
        warn ".env file not found, copying from .env.example"
        cp "$DEPLOY_DIR/.env.example" "$ENV_FILE"
    fi

    cd "$DEPLOY_DIR"
    docker compose -f "$STATEFUL_FILE" --env-file "$ENV_FILE" up -d 2>&1 || {
        fail "docker compose up (stateful) failed"
        exit 1
    }
    log "Stateful stack started, waiting 10s..."
    sleep 10

    docker compose -f "$COMPOSE_FILE" --env-file "$ENV_FILE" up -d 2>&1 || {
        fail "docker compose up (stateless) failed"
        exit 1
    }
    log "Stateless stack started, waiting ${WAIT_SECONDS}s for services to initialize..."
    sleep "$WAIT_SECONDS"
else
    echo ""
    echo "--- Skipping deploy (using running stack) ---"
fi

#####################################
# 2. Check all containers
#####################################

echo ""
echo "--- Checking containers ---"

STATEFUL_CONTAINERS=(
    "postgres"
    "qdrant"
)

STATELESS_CONTAINERS=(
    "kioku-ollama"
    "kioku-hivemind"
    "kioku-vexa-api-gateway"
    "kioku-vexa-admin-api"
    "kioku-vexa-meeting-api"
    "kioku-vexa-agent-api"
    "kioku-vexa-transcription-service"
    "kioku-vexa-runtime-api-local"
    "kioku-runtime-router"
    "kioku-mcp"
    "kioku-vexa-tts-service"
    "kioku-vexa-redis"
    "kioku-vexa-minio"
)

for container in "${STATEFUL_CONTAINERS[@]}" "${STATELESS_CONTAINERS[@]}"; do
    if docker ps --format '{{.Names}}' | grep -q "^${container}$"; then
        STATUS=$(docker inspect --format='{{.State.Status}}' "$container" 2>/dev/null || echo "unknown")
        if [ "$STATUS" = "running" ]; then
            log "$container: running"
        else
            fail "$container: $STATUS (expected running)"
        fi
    else
        fail "$container: NOT FOUND"
    fi
done

#####################################
# 3. Service health checks (edge ports only)
#####################################

echo ""
echo "--- Checking edge service health ---"

check_http() {
    local name="$1" url="$2" expected_prefix="$3"
    local status
    status=$(curl -s -o /dev/null -w "%{http_code}" --max-time 10 "$url" 2>/dev/null || echo "000")
    case "$expected_prefix" in
        2xx) [ "$status" -ge 200 ] 2>/dev/null && [ "$status" -lt 300 ] 2>/dev/null && return 0 || return 1 ;;
        *)   [ "$status" = "$expected_prefix" ] && return 0 || return 1 ;;
    esac
}

check_http "Hivemind Health" "http://localhost:9100/health" "200" \
    && log "Hivemind Health: HTTP 200" || fail "Hivemind Health: unreachable or non-200 at :9100/health"

check_http "Vexa API Gateway" "http://localhost:8056/" "200" \
    && log "Vexa API Gateway: HTTP 200" || fail "Vexa API Gateway: unreachable or non-200 at :8056/"

#####################################
# 4. Internal service health (via docker exec)
#####################################

echo ""
echo "--- Checking internal services ---"

if docker exec postgres pg_isready -U kioku -d kioku >/dev/null 2>&1; then
    log "Postgres: accepting connections"
else
    fail "Postgres: not accepting connections"
fi

if docker exec kioku-vexa-redis redis-cli ping 2>/dev/null | grep -qF PONG; then
    log "Redis: PONG"
else
    fail "Redis: no PONG"
fi

check_http "Vexa Admin API" "http://vexa-admin-api:8001/admin/health" "2xx" \
    && log "Vexa Admin API: healthy" \
    || warn "Vexa Admin API: health endpoint may not exist at /admin/health"

QDRANT_RESP=$(docker exec kioku-hivemind curl -s "http://qdrant:6334/collections" --max-time 5 2>/dev/null || echo '{"error":"unreachable"}')
if echo "$QDRANT_RESP" | python3 -c "import sys,json; d=json.load(sys.stdin); assert 'result' in d" 2>/dev/null; then
    log "Qdrant: responding with collections"
else
    warn "Qdrant: not yet reachable from hivemind container"
fi

#####################################
# 5. Hivemind API tests
#####################################

echo ""
echo "--- Testing Hivemind API ---"

HIVEMIND_URL="http://localhost:9100"

ADMIN_EMAIL="smoketest_admin_$(date +%s%N)@example.com"
COMPANY_SLUG="smoke-$(date +%s)"
REGISTER_RESP=$(curl -s -X POST "$HIVEMIND_URL/auth/register/admin" \
    -H "Content-Type: application/json" \
    -d "{\"company_name\":\"$COMPANY_SLUG\",\"email\":\"$ADMIN_EMAIL\",\"name\":\"Admin\",\"password\":\"testpassword123\"}" \
    --max-time 10)

TOKEN=$(echo "$REGISTER_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin).get('token',''))" 2>/dev/null || echo "")

if [ -n "$TOKEN" ]; then
    log "Auth: register admin success, got token"
else
    fail "Auth: register admin failed - $REGISTER_RESP"
fi

if [ -n "$TOKEN" ]; then
    ME_STATUS=$(curl -s -o /dev/null -w "%{http_code}" "$HIVEMIND_URL/auth/me" \
        -H "Authorization: Bearer $TOKEN" --max-time 10)
    [ "$ME_STATUS" = "200" ] && log "Auth: /me returns 200" || fail "Auth: /me returns $ME_STATUS"

    CFG_STATUS=$(curl -s -o /dev/null -w "%{http_code}" "$HIVEMIND_URL/company/config" \
        -H "Authorization: Bearer $TOKEN" --max-time 10)
    [ "$CFG_STATUS" = "200" ] && log "Company: GET /config returns 200" || fail "Company: GET /config returns $CFG_STATUS"

    SESS_RESP=$(curl -s -X POST "$HIVEMIND_URL/sessions" \
        -H "Authorization: Bearer $TOKEN" \
        -H "Content-Type: application/json" \
        -d '{"title":"Smoke Test Session","mode":"research"}' \
        --max-time 10)
    SESS_ID=$(echo "$SESS_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin).get('id',''))" 2>/dev/null || echo "")

    if [ -n "$SESS_ID" ]; then
        log "Sessions: create session success (id=$SESS_ID)"

        LIST_STATUS=$(curl -s -o /dev/null -w "%{http_code}" "$HIVEMIND_URL/sessions" \
            -H "Authorization: Bearer $TOKEN" --max-time 10)
        [ "$LIST_STATUS" = "200" ] && log "Sessions: list returns 200" || fail "Sessions: list returns $LIST_STATUS"

        GET_STATUS=$(curl -s -o /dev/null -w "%{http_code}" "$HIVEMIND_URL/sessions/$SESS_ID" \
            -H "Authorization: Bearer $TOKEN" --max-time 10)
        [ "$GET_STATUS" = "200" ] && log "Sessions: get returns 200" || fail "Sessions: get returns $GET_STATUS"

        DEL_STATUS=$(curl -s -o /dev/null -w "%{http_code}" -X DELETE "$HIVEMIND_URL/sessions/$SESS_ID" \
            -H "Authorization: Bearer $TOKEN" --max-time 10)
        [ "$DEL_STATUS" = "200" ] && log "Sessions: delete returns 200" || fail "Sessions: delete returns $DEL_STATUS"
    else
        fail "Sessions: create session failed"
    fi

    KNN_STATUS=$(curl -s -o /dev/null -w "%{http_code}" -X POST "$HIVEMIND_URL/knowledge/search" \
        -H "Authorization: Bearer $TOKEN" \
        -H "Content-Type: application/json" \
        -d '{"query":"test","limit":3}' \
        --max-time 10)
    [ "$KNN_STATUS" = "200" ] && log "Knowledge: search returns 200" || fail "Knowledge: search returns $KNN_STATUS"

    NOW_MS=$(($(date +%s) * 1000))
    MTG_STATUS=$(curl -s -o /dev/null -w "%{http_code}" -X POST "$HIVEMIND_URL/meetings" \
        -H "Authorization: Bearer $TOKEN" \
        -H "Content-Type: application/json" \
        -d "{\"title\":\"Test Meeting\",\"date\":$NOW_MS,\"duration_seconds\":600,\"participants\":[\"Alice\"],\"transcript\":[{\"speaker\":\"Alice\",\"text\":\"Hello\",\"start_time\":0,\"end_time\":2}]}" \
        --max-time 10)
    [ "$MTG_STATUS" = "200" ] && log "Meetings: ingest returns 200" || fail "Meetings: ingest returns $MTG_STATUS"

    OUT_STATUS=$(curl -s -o /dev/null -w "%{http_code}" -X POST "$HIVEMIND_URL/auth/signout" \
        -H "Authorization: Bearer $TOKEN" --max-time 10)
    [ "$OUT_STATUS" = "200" ] && log "Auth: signout returns 200" || fail "Auth: signout returns $OUT_STATUS"
fi

#####################################
# 6. Vexa API tests
#####################################

echo ""
echo "--- Testing Vexa API Gateway ---"

VEXA_URL="http://localhost:8056"
VEXA_KEY="${VEXA_ADMIN_API_TOKEN:-token}"

UNAUTH_STATUS=$(curl -s -o /dev/null -w "%{http_code}" -X POST "$VEXA_URL/bots" \
    -H "Content-Type: application/json" \
    -d '{"platform":"google_meet","native_meeting_id":"test"}' \
    --max-time 10)
if [ "$UNAUTH_STATUS" = "403" ] || [ "$UNAUTH_STATUS" = "401" ]; then
    log "Vexa: unauthenticated bot request rejected ($UNAUTH_STATUS)"
else
    warn "Vexa: unauthenticated bot request returned $UNAUTH_STATUS (expected 401/403)"
fi

BOTS_STATUS=$(curl -s -o /dev/null -w "%{http_code}" "$VEXA_URL/bots" \
    -H "X-API-Key: $VEXA_KEY" --max-time 10)
if [ "$BOTS_STATUS" = "200" ]; then
    log "Vexa: list bots returns 200"
else
    warn "Vexa: list bots returns $BOTS_STATUS (voice stack may need more time)"
fi

#####################################
# 7. DB schema check
#####################################

echo ""
echo "--- Checking database schemas ---"

SCHEMAS=$(docker exec postgres psql -U kioku -d kioku -t -c \
    "SELECT schema_name FROM information_schema.schemata WHERE schema_name IN ('public','hivemind','vexa') ORDER BY schema_name;" 2>/dev/null || echo "")

for schema in public hivemind vexa; do
    if echo "$SCHEMAS" | grep -q "$schema"; then
        log "Schema '$schema': exists"
    else
        warn "Schema '$schema': not found"
    fi
done

TABLE_COUNT=$(docker exec postgres psql -U kioku -d kioku -t -c \
    "SELECT count(*) FROM pg_tables WHERE schemaname IN ('public','hivemind','vexa');" 2>/dev/null | tr -d ' ')
log "Total tables across hivemind/public/vexa schemas: $TABLE_COUNT"

#####################################
# Summary
#####################################

echo ""
echo "========================================="
if [ "$FAIL_COUNT" -eq 0 ]; then
    echo -e "${GREEN}ALL TESTS PASSED${NC}"
else
    echo -e "${RED}$FAIL_COUNT TEST(S) FAILED${NC}"
fi
echo "========================================="

exit $FAIL_COUNT
