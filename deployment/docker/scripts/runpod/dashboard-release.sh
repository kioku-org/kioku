#!/usr/bin/env bash
# BASH_ENV hook for pods whose image/template predates DASHBOARD_RELEASE_DIR.
# The first startup shell selects the durable dashboard build before supervisor
# starts. Later noninteractive shells leave the selected build in place.
if [[ -n "${DASHBOARD_RELEASE_DIR:-}" ]]; then
    if [[ ! -f "$DASHBOARD_RELEASE_DIR/server.js" || ! -f "$DASHBOARD_RELEASE_DIR/.next/BUILD_ID" ]]; then
        echo "[KIOKU] Selected dashboard release is incomplete" >&2
        exit 1
    fi
    if [[ "$(readlink /opt/dashboard || true)" != "$DASHBOARD_RELEASE_DIR" ]]; then
        if [[ -d /opt/dashboard && ! -L /opt/dashboard ]]; then
            mv /opt/dashboard /opt/dashboard-image
        else
            rm -f /opt/dashboard
        fi
        ln -s "$DASHBOARD_RELEASE_DIR" /opt/dashboard
    fi
fi
