#!/usr/bin/env bash
# Remove a RunPod pod by ID.
set -euo pipefail

POD_ID="${1:-}"

if [[ -z "$POD_ID" ]]; then
    echo "Usage: $0 <pod-id>"
    echo ""
    echo "List pods: runpodctl get pod"
    exit 1
fi

read -rp "Remove pod $POD_ID? (y/N) " confirm
[[ "$confirm" =~ ^[Yy]$ ]] || { echo "Aborted."; exit 0; }

runpodctl remove pod "$POD_ID"
echo "Pod $POD_ID removed."
