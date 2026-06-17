#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

[[ -f "$SCRIPT_DIR/.env" ]] || { echo ".env not found"; exit 1; }

set -a
while IFS='=' read -r key value; do
    [[ "$key" =~ ^#.*$ || -z "$key" ]] && continue
    value="${value%\"}"
    value="${value#\"}"
    export "$key"="$value"
done < "$SCRIPT_DIR/.env"
set +a

[[ -n "${RUNPOD_API_KEY:-}" ]] || { echo "RUNPOD_API_KEY not set"; exit 1; }

if [[ -f "$SCRIPT_DIR/.pod-info" ]]; then
    source "$SCRIPT_DIR/.pod-info"
    echo "Pod ID: $POD_ID"
    echo ""
    curl -s "https://rest.runpod.io/v1/pods/$POD_ID" \
        -H "Authorization: Bearer $RUNPOD_API_KEY" | python3 -m json.tool
else
    echo "Listing all pods:"
    echo ""
    curl -s "https://rest.runpod.io/v1/pods" \
        -H "Authorization: Bearer $RUNPOD_API_KEY" | python3 -m json.tool
fi
