#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

info()  { echo -e "${GREEN}[INFO]${NC} $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC} $*"; }
error() { echo -e "${RED}[ERROR]${NC} $*"; exit 1; }

[[ -f "$SCRIPT_DIR/.env" ]] || error ".env not found"
set -a
. <(sed 's/#.*//g' "$SCRIPT_DIR/.env" | grep -v '^

POD_ID="${1:-}"

# Try to load from .pod-info if no argument
if [[ -z "$POD_ID" && -f "$SCRIPT_DIR/.pod-info" ]]; then
    source "$SCRIPT_DIR/.pod-info"
    POD_ID="${POD_ID:-}"
fi

[[ -n "$POD_ID" ]] || error "Usage: $0 <pod-id>"

info "Terminating Pod: $POD_ID"
read -rp "Are you sure? (y/N) " confirm
[[ "$confirm" =~ ^[Yy]$ ]] || { info "Aborted"; exit 0; }

RESPONSE=$(curl -s -X DELETE "https://rest.runpod.io/v1/pods/$POD_ID" \
    -H "Authorization: Bearer $RUNPOD_API_KEY")

if echo "$RESPONSE" | grep -q '"code"'; then
    error "API error: $RESPONSE"
fi

info "Pod $POD_ID terminated"
rm -f "$SCRIPT_DIR/.pod-info"
)
set +a
[[ -n "${RUNPOD_API_KEY:-}" ]] || error "RUNPOD_API_KEY not set"

POD_ID="${1:-}"

# Try to load from .pod-info if no argument
if [[ -z "$POD_ID" && -f "$SCRIPT_DIR/.pod-info" ]]; then
    source "$SCRIPT_DIR/.pod-info"
    POD_ID="${POD_ID:-}"
fi

[[ -n "$POD_ID" ]] || error "Usage: $0 <pod-id>"

info "Terminating Pod: $POD_ID"
read -rp "Are you sure? (y/N) " confirm
[[ "$confirm" =~ ^[Yy]$ ]] || { info "Aborted"; exit 0; }

RESPONSE=$(curl -s -X DELETE "https://rest.runpod.io/v1/pods/$POD_ID" \
    -H "Authorization: Bearer $RUNPOD_API_KEY")

if echo "$RESPONSE" | grep -q '"code"'; then
    error "API error: $RESPONSE"
fi

info "Pod $POD_ID terminated"
rm -f "$SCRIPT_DIR/.pod-info"
