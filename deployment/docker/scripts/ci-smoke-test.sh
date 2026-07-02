#!/usr/bin/env bash
# HTTP-only smoke test — runs against remote URLs (RunPod or any deployment).
# Usage: ci-smoke-test.sh <hivemind_url> <vexa_url> <vexa_admin_token>
set -euo pipefail

HIVEMIND_URL="${1:?Usage: $0 <hivemind_url> <vexa_url> <vexa_admin_token>}"
VEXA_URL="${2:?}"
VEXA_ADMIN_TOKEN="${3:?}"

RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m'

FAIL_COUNT=0
log()  { echo -e "${GREEN}[PASS]${NC} $1"; }
fail() { echo -e "${RED}[FAIL]${NC} $1"; FAIL_COUNT=$((FAIL_COUNT + 1)); }

http() {
  curl -sf -o /dev/null -w "%{http_code}" --max-time 15 "$@" 2>/dev/null || echo "000"
}

echo "==============================="
echo "  Kioku CI Smoke Test"
echo "  Hivemind : $HIVEMIND_URL"
echo "  Vexa     : $VEXA_URL"
echo "==============================="
echo ""

# ── Health checks ────────────────────────────────────────────────────────────
echo "--- Health ---"

STATUS=$(http "$HIVEMIND_URL/health")
[ "$STATUS" = "200" ] && log "Hivemind /health: 200" || fail "Hivemind /health: $STATUS"

STATUS=$(http "$VEXA_URL/")
[ "$STATUS" = "200" ] && log "Vexa gateway /: 200" || fail "Vexa gateway /: $STATUS"

# ── Auth flow ─────────────────────────────────────────────────────────────────
echo ""
echo "--- Auth ---"

SLUG="ci-$(date +%s)"
EMAIL="${SLUG}@test.kioku"

REGISTER_RESP=$(curl -sf -X POST "$HIVEMIND_URL/auth/register/admin" \
  -H "Content-Type: application/json" \
  -d "{\"workspace_name\":\"$SLUG\",\"email\":\"$EMAIL\",\"name\":\"CI\",\"password\":\"test1234\"}" \
  --max-time 15 2>/dev/null || echo "{}")

TOKEN=$(echo "$REGISTER_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin).get('token',''))" 2>/dev/null || echo "")
[ -n "$TOKEN" ] && log "Auth: register+token OK" || fail "Auth: register failed — $REGISTER_RESP"

if [ -n "$TOKEN" ]; then
  STATUS=$(http "$HIVEMIND_URL/auth/me" -H "Authorization: Bearer $TOKEN")
  [ "$STATUS" = "200" ] && log "Auth: /me 200" || fail "Auth: /me $STATUS"

  STATUS=$(http "$HIVEMIND_URL/workspace/config" -H "Authorization: Bearer $TOKEN")
  [ "$STATUS" = "200" ] && log "Workspace: /config 200" || fail "Workspace: /config $STATUS"

  # ── Sessions ───────────────────────────────────────────────────────────────
  echo ""
  echo "--- Sessions ---"

  SESS_RESP=$(curl -sf -X POST "$HIVEMIND_URL/sessions" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d "{\"title\":\"CI Test\",\"mode\":\"research\"}" \
    --max-time 15 2>/dev/null || echo "{}")

  SESS_ID=$(echo "$SESS_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin).get('id',''))" 2>/dev/null || echo "")
  [ -n "$SESS_ID" ] && log "Sessions: create OK (id=$SESS_ID)" || fail "Sessions: create failed"

  STATUS=$(http "$HIVEMIND_URL/sessions" -H "Authorization: Bearer $TOKEN")
  [ "$STATUS" = "200" ] && log "Sessions: list 200" || fail "Sessions: list $STATUS"

  if [ -n "$SESS_ID" ]; then
    STATUS=$(http "$HIVEMIND_URL/sessions/$SESS_ID" -H "Authorization: Bearer $TOKEN")
    [ "$STATUS" = "200" ] && log "Sessions: get 200" || fail "Sessions: get $STATUS"

    STATUS=$(http -X DELETE "$HIVEMIND_URL/sessions/$SESS_ID" -H "Authorization: Bearer $TOKEN")
    [ "$STATUS" = "200" ] && log "Sessions: delete 200" || fail "Sessions: delete $STATUS"
  fi

  # ── Knowledge ─────────────────────────────────────────────────────────────
  echo ""
  echo "--- Knowledge ---"

  STATUS=$(http -X POST "$HIVEMIND_URL/knowledge/search" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"query":"test","limit":3}')
  [ "$STATUS" = "200" ] && log "Knowledge: search 200" || fail "Knowledge: search $STATUS"

  # ── Meetings ingest ───────────────────────────────────────────────────────
  echo ""
  echo "--- Meetings ---"

  NOW_MS=$(python3 -c "import time; print(int(time.time() * 1000))")
  STATUS=$(http -X POST "$HIVEMIND_URL/meetings" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d "{\"title\":\"CI Meeting\",\"date\":$NOW_MS,\"duration_seconds\":60,\"participants\":[\"Alice\"],\"transcript\":[{\"speaker\":\"Alice\",\"text\":\"Hello\",\"start_time\":0,\"end_time\":2}]}")
  [ "$STATUS" = "200" ] && log "Meetings: ingest 200" || fail "Meetings: ingest $STATUS"
fi

# ── Vexa auth guard ───────────────────────────────────────────────────────────
echo ""
echo "--- Vexa ---"

STATUS=$(http -X POST "$VEXA_URL/bots" \
  -H "Content-Type: application/json" \
  -d '{"platform":"google_meet","native_meeting_id":"test"}')
{ [ "$STATUS" = "401" ] || [ "$STATUS" = "403" ]; } \
  && log "Vexa: unauthenticated request rejected ($STATUS)" \
  || fail "Vexa: expected 401/403, got $STATUS"

STATUS=$(http "$VEXA_URL/bots" -H "X-API-Key: $VEXA_ADMIN_TOKEN")
[ "$STATUS" = "200" ] && log "Vexa: list bots 200" || fail "Vexa: list bots $STATUS"

# ── Summary ───────────────────────────────────────────────────────────────────
echo ""
echo "==============================="
if [ "$FAIL_COUNT" -eq 0 ]; then
  echo -e "${GREEN}ALL TESTS PASSED${NC}"
else
  echo -e "${RED}$FAIL_COUNT TEST(S) FAILED${NC}"
fi
echo "==============================="
exit $FAIL_COUNT
