#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(dirname "$SCRIPT_DIR")"

GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m'

pass() { echo -e "${GREEN}PASS${NC} $*"; }
fail() { echo -e "${RED}FAIL${NC} $*"; FAIL=$((FAIL + 1)); }

FAIL=0

echo "═══════════════════════════════════════════"
echo "  Kioku Test Suite"
echo "═══════════════════════════════════════════"
echo ""

# ─── CLI Unit Tests ───────────────────────────────────────────────────────────
echo "── CLI Unit Tests ──"
cd "$ROOT/apps/cli"
if cargo test -p cc-auth -p cc-kioku 2>&1 | grep "test result:" | grep -v "0 passed; 0 failed" | grep -q "ok"; then
    pass "CLI unit tests (cc-auth, cc-kioku)"
else
    fail "CLI unit tests"
fi

# ─── Hivemind Tests ────────────────────────────────────────────────────────────
echo "── Hivemind Tests ──"
cd "$ROOT/services/hivemind"

if cargo check 2>&1 | tail -1 | grep -q "Finished"; then
    pass "Hivemind compiles"
else
    fail "Hivemind compile check"
fi

# Integration tests require a running server
if curl -sf http://localhost:9100/health >/dev/null 2>&1; then
    if cargo test 2>&1 | tail -5 | grep -q "test result: ok"; then
        pass "Hivemind integration tests"
    else
        fail "Hivemind integration tests"
    fi
else
    echo "  SKIP: Hivemind integration tests (server not running on :9100)"
    echo "        Start with: cd deployment/docker && ./scripts/manage.sh start"
fi

# ─── Docker Stack Health ─────────────────────────────────────────────────────
echo ""
echo "── Docker Stack ──"
if command -v docker &>/dev/null && docker ps --format '{{.Names}}' | grep -q '^kioku-'; then
    "$ROOT/deployment/docker/scripts/healthcheck.sh"
else
    echo "  SKIP: Docker stack not running"
    echo "        Start with: cd deployment/docker && ./scripts/manage.sh start"
fi

echo ""
echo "═══════════════════════════════════════════"
if [ $FAIL -eq 0 ]; then
    echo -e "  ${GREEN}ALL TESTS PASSED${NC}"
else
    echo -e "  ${RED}$FAIL TEST(S) FAILED${NC}"
fi
echo "═══════════════════════════════════════════"
exit $FAIL